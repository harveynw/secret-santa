use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fs, io, path::Path};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: Uuid,
    pub name: String,
    pub email: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub participants: Vec<Entry>,
}
impl Selection {
    pub fn save_new(&self, path: &Path) -> io::Result<()> {
        use std::io::Write;
        self.validate()?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        let tmp = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let result = (|| {
            let mut file = options.open(&tmp)?;
            file.write_all(&serde_json::to_vec_pretty(self)?)?;
            file.sync_all()?;
            fs::hard_link(&tmp, path)?;
            fs::File::open(parent)?.sync_all()
        })();
        let _ = fs::remove_file(tmp);
        result
    }
    pub fn load(path: &Path) -> io::Result<Self> {
        let selection: Self = serde_json::from_slice(&fs::read(path)?)?;
        selection.validate()?;
        Ok(selection)
    }
    pub fn validate(&self) -> io::Result<()> {
        let mut ids = HashSet::new();
        let mut emails = HashSet::new();
        if self.participants.len() < 2
            || self.participants.iter().any(|p| {
                p.id.is_nil()
                    || !ids.insert(p.id)
                    || !emails.insert(p.email.to_lowercase())
                    || p.name.trim().is_empty()
                    || p.name.chars().any(char::is_control)
                    || !email_address::EmailAddress::is_valid(&p.email)
            })
        {
            return Err(io::Error::other(
                "Selection requires at least two valid, unique participants",
            ));
        }
        Ok(())
    }
    pub fn recipient(&self, id: Uuid) -> Option<&Entry> {
        self.participants
            .iter()
            .position(|p| p.id == id)
            .map(|i| &self.participants[(i + 1) % self.participants.len()])
    }
}
pub fn path() -> std::path::PathBuf {
    crate::config::SELECTION_PATH.into()
}
