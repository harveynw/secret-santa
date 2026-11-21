use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Database {
    pub closed: bool,
    // Preserve legacy private data when reading earlier project databases.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub drawn: bool,
    pub participants: Vec<Participant>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assignments: Vec<(Uuid, Uuid)>,
    pub mail: Vec<Mail>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reveal_mail: Vec<Mail>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Participant {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub verified: bool,
    pub token_hash: String,
    pub expires_at: u64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Mail {
    pub id: Uuid,
    pub to: String,
    pub subject: String,
    pub text: String,
    pub accepted: bool,
}
pub struct Store {
    pub db: Database,
    path: PathBuf,
    _lock: File,
}
impl Store {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        let lock = private_file(&path.with_extension("lock"))?;
        lock.try_lock_exclusive()?;
        let db = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Database::default(),
            Err(e) => return Err(e),
        };
        Ok(Self {
            db,
            path,
            _lock: lock,
        })
    }
    // Write candidate state before publishing it in memory. Never reset on parse errors.
    pub fn commit(&mut self, next: Database) -> io::Result<()> {
        let tmp = self.path.with_extension("tmp");
        let mut file = private_file(&tmp)?;
        file.set_len(0)?;
        file.write_all(&serde_json::to_vec_pretty(&next)?)?;
        file.sync_all()?;
        fs::rename(&tmp, &self.path)?;
        // After rename, memory must agree with the visible file even if directory sync fails.
        self.db = next;
        let parent = self
            .path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        File::open(parent)?.sync_all()
    }
}
fn private_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
