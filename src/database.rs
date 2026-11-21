use crate::Result;
use rusqlite::{Connection, MAIN_DB, params};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub fn location() -> Result<PathBuf> {
    let local =
        std::env::var_os("LOCALAPPDATA").ok_or("Windows local app data folder is unavailable.")?;
    let dir = PathBuf::from(local).join("QuickVault");
    fs::create_dir_all(&dir)?;
    Ok(dir.join("quickvault.sqlite3"))
}

pub struct Vault(Connection);

#[derive(Debug, PartialEq)]
pub struct Snippet {
    pub title: String,
    pub content: String,
    pub category: String,
    pub pinned: bool,
}

pub struct Hit {
    pub id: i64,
    pub title: String,
    pub preview: String,
    pub category: String,
    pub pinned: bool,
}

impl Vault {
    pub fn open(path: &Path) -> Result<Self> {
        let mut db = Connection::open(path)?;
        db.busy_timeout(Duration::from_secs(2))?;
        let version = db.query_row("PRAGMA user_version", [], |r| r.get::<_, i32>(0))?;
        match version {
            0 => {
                let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                tx.execute_batch("CREATE TABLE IF NOT EXISTS snippets (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, content TEXT NOT NULL,
                    category TEXT NOT NULL, pinned INTEGER NOT NULL CHECK(pinned IN (0,1)),
                    created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, revision INTEGER NOT NULL DEFAULT 1);
                    CREATE TABLE IF NOT EXISTS settings (name TEXT PRIMARY KEY, value TEXT NOT NULL);
                    PRAGMA user_version=1;")?;
                tx.commit()?;
            }
            1 => {}
            _ => return Err("This vault requires a newer version of QuickVault.".into()),
        }
        Ok(Self(db))
    }
    pub fn save(&self, existing: Option<(i64, i64)>, item: &Snippet) -> Result<i64> {
        if item.title.trim().is_empty() || item.content.is_empty() {
            return Err("Enter a title and some content.".into());
        }
        if [&item.title, &item.content, &item.category]
            .iter()
            .any(|s| s.contains('\0'))
        {
            return Err("Text cannot contain a null character.".into());
        }
        let values = params![
            item.title.trim(),
            item.content,
            item.category.trim(),
            item.pinned
        ];
        if let Some((id, revision)) = existing {
            let rows = self.0.execute(
                "UPDATE snippets SET title=?1, content=?2, category=?3, pinned=?4,
                updated_at=unixepoch(), revision=revision+1 WHERE id=?5 AND revision=?6",
                params![
                    item.title.trim(),
                    item.content,
                    item.category.trim(),
                    item.pinned,
                    id,
                    revision
                ],
            )?;
            if rows == 0 {
                return Err("The snippet changed or was removed. Open it again.".into());
            }
            Ok(id)
        } else {
            self.0.execute(
                "INSERT INTO snippets(title,content,category,pinned,created_at,updated_at)
                VALUES(?1,?2,?3,?4,unixepoch(),unixepoch())",
                values,
            )?;
            Ok(self.0.last_insert_rowid())
        }
    }
    pub fn get(&self, id: i64) -> Result<(Snippet, i64)> {
        Ok(self.0.query_row(
            "SELECT title,content,category,pinned,revision FROM snippets WHERE id=?1",
            [id],
            |r| {
                Ok((
                    Snippet {
                        title: r.get(0)?,
                        content: r.get(1)?,
                        category: r.get(2)?,
                        pinned: r.get(3)?,
                    },
                    r.get(4)?,
                ))
            },
        )?)
    }
    pub fn delete(&self, id: i64, revision: i64) -> Result<()> {
        if self.0.execute(
            "DELETE FROM snippets WHERE id=?1 AND revision=?2",
            params![id, revision],
        )? == 0
        {
            return Err("The snippet changed or was removed. Open it again.".into());
        }
        Ok(())
    }
    pub fn search(&self, term: &str, offset: usize, limit: usize) -> Result<Vec<Hit>> {
        let mut query = self.0.prepare(
            "SELECT id,title,substr(content,1,80),category,pinned FROM snippets
            WHERE ?1='' OR instr(lower(title),lower(?1))>0 OR instr(lower(content),lower(?1))>0
            ORDER BY pinned DESC,title COLLATE NOCASE,id LIMIT ?2 OFFSET ?3",
        )?;
        let rows = query.query_map(params![term, limit as i64, offset as i64], |r| {
            Ok(Hit {
                id: r.get(0)?,
                title: r.get(1)?,
                preview: r.get(2)?,
                category: r.get(3)?,
                pinned: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
    pub fn prepare_examples(&mut self) -> Result<()> {
        let tx = self
            .0
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let first = tx.execute(
            "INSERT OR IGNORE INTO settings(name,value) VALUES('starter_examples','2')",
            [],
        )?;
        let used: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_sequence WHERE name='snippets') OR EXISTS(SELECT 1 FROM snippets)", [], |r| r.get(0))?;
        if first == 1 && !used {
            for (title, content, category) in [
                ("Email", "you@example.com", "Contact"),
                ("GitHub", "https://github.com/yourname", "Profile"),
                ("LinkedIn", "https://linkedin.com/in/yourname", "Profile"),
                ("Portfolio", "https://yourportfolio.com", "Website"),
            ] {
                tx.execute("INSERT INTO snippets(title,content,category,pinned,created_at,updated_at) VALUES(?1,?2,?3,0,unixepoch(),unixepoch())", params![title,content,category])?;
            }
        }
        // The previous first-run bundle inserted this example fifth in an empty vault.
        if tx.execute(
            "UPDATE settings SET value='2' WHERE name='starter_examples' AND value='1'",
            [],
        )? == 1
        {
            tx.execute("DELETE FROM snippets WHERE id=5 AND title='Python Command'
                AND content='python manage.py runserver' AND category='Command' AND revision=1 AND pinned=0", [])?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn count(&self) -> Result<usize> {
        Ok(self
            .0
            .query_row("SELECT count(*) FROM snippets", [], |r| r.get(0))?)
    }
    pub fn setting(&self, name: &str, default: &str) -> Result<String> {
        use rusqlite::OptionalExtension;
        Ok(self
            .0
            .query_row("SELECT value FROM settings WHERE name=?1", [name], |r| {
                r.get(0)
            })
            .optional()?
            .unwrap_or_else(|| default.to_owned()))
    }
    pub fn set(&self, name: &str, value: &str) -> Result<()> {
        self.0.execute(
            "INSERT INTO settings(name,value) VALUES(?1,?2)
            ON CONFLICT(name) DO UPDATE SET value=excluded.value",
            params![name, value],
        )?;
        Ok(())
    }
    pub fn backup(&self, path: &Path) -> Result<()> {
        // Reserve the destination exclusively so a typo cannot overwrite another vault.
        let reserved = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        drop(reserved);
        if let Err(e) = self.0.backup(MAIN_DB, path, None) {
            let _ = fs::remove_file(path);
            return Err(e.into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn examples_only_fill_a_never_used_vault() -> Result<()> {
        let mut db = Vault::open(Path::new(":memory:"))?;
        db.prepare_examples()?;
        assert_eq!(db.count()?, 4);
        for hit in db.search("", 0, 5)? {
            db.delete(hit.id, 1)?;
        }
        db.prepare_examples()?;
        assert_eq!(db.count()?, 0);
        let mut existing = Vault::open(Path::new(":memory:"))?;
        let id = existing.save(None, &snippet("Mine", "Original"))?;
        existing.prepare_examples()?;
        assert_eq!(existing.count()?, 1);
        assert_eq!(existing.get(id)?.0.content, "Original");
        Ok(())
    }
    fn snippet(title: &str, content: &str) -> Snippet {
        Snippet {
            title: title.into(),
            content: content.into(),
            category: "work".into(),
            pinned: false,
        }
    }
    #[test]
    fn remove_only_the_original_unedited_python_example() -> Result<()> {
        for edited in [false, true] {
            let mut db = Vault::open(Path::new(":memory:"))?;
            db.prepare_examples()?;
            db.set("starter_examples", "1")?;
            let python = Snippet {
                title: "Python Command".into(),
                content: "python manage.py runserver".into(),
                category: "Command".into(),
                pinned: false,
            };
            let old = db.save(None, &python)?;
            if edited {
                db.save(Some((old, 1)), &python)?;
            }
            let personal = db.save(None, &python)?;
            db.prepare_examples()?;
            assert_eq!(db.get(old).is_ok(), edited);
            assert_eq!(db.get(personal)?.0, python);
            db.prepare_examples()?;
            assert_eq!(db.count()?, if edited { 6 } else { 5 });
        }
        Ok(())
    }
    #[test]
    fn exact_content_order_search_and_stale_edits() -> Result<()> {
        let db = Vault::open(Path::new(":memory:"))?;
        let item = snippet("Alpha", "  first\r\n%s_' 🦀  ");
        let a = db.save(None, &item)?;
        let mut other = snippet("Zulu", "other");
        other.pinned = true;
        let z = db.save(None, &other)?;
        assert_eq!(db.search("", 0, 1)?[0].id, z);
        assert_eq!(db.search("", 1, 1)?[0].id, a);
        assert_eq!(db.search("%S_'", 0, 10)?[0].id, a);
        assert_eq!(db.search("ALPHA", 0, 10)?[0].id, a);
        let (saved, rev) = db.get(a)?;
        assert_eq!(saved, item);
        db.save(Some((a, rev)), &other)?;
        assert!(db.save(Some((a, rev)), &item).is_err());
        assert!(db.delete(a, rev).is_err());
        db.delete(a, rev + 1)?;
        assert!(db.get(a).is_err());
        Ok(())
    }
    #[test]
    fn backup_survives_restart_and_rejects_overwrite() -> Result<()> {
        let dir = std::env::temp_dir().join(format!(
            "quickvault-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        fs::create_dir(&dir)?;
        let src = dir.join("original.db");
        let dest = dir.join("backup.db");
        {
            let db = Vault::open(&src)?;
            let id = db.save(None, &snippet("test", "example"))?;
            db.set("auto_hide", "0")?;
            db.backup(&dest)?;
            assert!(db.backup(&dest).is_err());
            drop(db);
            for file in [&src, &dest] {
                let copy = Vault::open(file)?;
                assert_eq!(copy.get(id)?.0.content, "example");
                assert_eq!(copy.setting("auto_hide", "1")?, "0");
            }
        }
        fs::remove_file(src)?;
        fs::remove_file(dest)?;
        fs::remove_dir(dir)?;
        Ok(())
    }
    #[test]
    fn damaged_database_is_preserved() -> Result<()> {
        let p = std::env::temp_dir().join(format!("quickvault-damaged-{}.db", std::process::id()));
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&p)?;
        use std::io::Write;
        f.write_all(b"not a sqlite database")?;
        drop(f);
        assert!(Vault::open(&p).is_err());
        assert_eq!(fs::read(&p)?, b"not a sqlite database");
        fs::remove_file(p)?;
        Ok(())
    }
}
