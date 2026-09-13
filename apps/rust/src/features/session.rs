//! Persistência local e versionada de sessões do `prt desc`.
//!
//! O formato deste módulo é deliberadamente pequeno: ele guarda somente o
//! material necessário para revisar e retomar uma publicação. Credenciais,
//! configuração, prompt e contexto bruto do Git ficam fora do snapshot.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::ConfigPaths;
use crate::error::{AppError, Result};
use crate::git::{GitContextFingerprint, RepositoryRemote};

/// Versão do formato persistido.
pub const SCHEMA_VERSION: u32 = 1;
const SESSIONS_DIR: &str = "sessions";
const FILE_PREFIX: &str = "session-";
const FILE_SUFFIX: &str = ".json";
const LOCK_SUFFIX: &str = ".lock";

/// Estado durável de um target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TargetState {
    /// Nenhuma chamada remota começou.
    Pending,
    /// A chamada começou, mas o resultado não é seguro para repetir.
    AttemptingOrUncertain {
        /// Mensagem segura para orientar a reconciliação.
        message: Option<String>,
    },
    /// O PR foi confirmado ou adotado explicitamente.
    Confirmed {
        /// ID do PR no Azure DevOps.
        id: i64,
        /// URL do PR confirmado.
        url: String,
    },
    /// O provider confirmou que a criação falhou.
    Failed {
        /// Mensagem segura da falha.
        message: String,
    },
}

impl TargetState {
    /// Nome estável exposto na UI e no diagnóstico.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::AttemptingOrUncertain { .. } => "attempting_or_uncertain",
            Self::Confirmed { .. } => "confirmed",
            Self::Failed { .. } => "failed",
        }
    }

    /// Retorna se o target já tem uma receipt final.
    #[must_use]
    pub const fn is_confirmed(&self) -> bool {
        matches!(self, Self::Confirmed { .. })
    }
}

/// Identidade remota sem qualquer credencial.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteIdentity {
    /// Organização Azure DevOps.
    pub organization: String,
    /// Projeto Azure DevOps.
    pub project: String,
    /// Repositório Azure DevOps.
    pub repository: String,
}

impl From<&RepositoryRemote> for RemoteIdentity {
    fn from(remote: &RepositoryRemote) -> Self {
        Self {
            organization: remote.organization.clone(),
            project: remote.project.clone(),
            repository: remote.repository.clone(),
        }
    }
}

/// Estado persistido de uma publicação por target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TargetSnapshot {
    /// Nome do target.
    pub target: String,
    /// Estado durável do target.
    pub state: TargetState,
}

/// Snapshot seguro de uma sessão `prt desc`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionSnapshot {
    /// Versão do schema.
    pub schema_version: u32,
    /// UUID v4 da sessão.
    pub session_id: String,
    /// Revisão monotônica do snapshot.
    pub revision: u64,
    /// Identidade absoluta do checkout.
    pub repository: String,
    /// Identidade remota sem PAT.
    pub remote: Option<RemoteIdentity>,
    /// Branch da origem.
    pub source_branch: String,
    /// Ref completa da origem.
    pub source_ref: String,
    /// OID da origem no momento da captura.
    pub source_oid: String,
    /// OIDs observados para cada target.
    pub target_oids: BTreeMap<String, String>,
    /// Título aprovado.
    pub title: String,
    /// Body aprovado.
    pub body: String,
    /// Work Item associado, vazio quando não há vínculo.
    pub work_item_id: String,
    /// Reviewer por target, na mesma ordem de `targets`.
    pub reviewers: Vec<String>,
    /// Targets e estados duráveis.
    pub targets: Vec<TargetSnapshot>,
    /// Data de criação em UTC RFC3339.
    pub created_at: String,
    /// Data da última revisão em UTC RFC3339.
    pub updated_at: String,
}

impl SessionSnapshot {
    /// Cria o snapshot inicial com todos os targets pendentes.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repository: String,
        remote: Option<&RepositoryRemote>,
        source_branch: String,
        source_ref: String,
        fingerprint: &GitContextFingerprint,
        title: String,
        body: String,
        work_item_id: String,
        reviewers: Vec<String>,
        targets: Vec<String>,
    ) -> Self {
        let timestamp = utc_now();
        Self {
            schema_version: SCHEMA_VERSION,
            session_id: Uuid::new_v4().to_string(),
            revision: 0,
            repository,
            remote: remote.map(RemoteIdentity::from),
            source_branch,
            source_ref,
            source_oid: fingerprint.source_oid.clone(),
            target_oids: fingerprint.target_oids.clone(),
            title,
            body,
            work_item_id,
            reviewers,
            targets: targets
                .into_iter()
                .map(|target| TargetSnapshot {
                    target,
                    state: TargetState::Pending,
                })
                .collect(),
            created_at: timestamp.clone(),
            updated_at: timestamp,
        }
    }

    /// Retorna o UUID v4 validado do snapshot.
    ///
    /// # Errors
    ///
    /// Retorna erro quando o ID não é um UUID v4.
    pub fn id(&self) -> Result<Uuid> {
        let id = Uuid::parse_str(&self.session_id).map_err(|_| invalid_session("UUID inválido"))?;
        if id.get_version_num() != 4 {
            return Err(invalid_session("a sessão não usa UUID v4"));
        }
        Ok(id)
    }

    /// Valida invariantes do schema antes de qualquer uso.
    ///
    /// # Errors
    ///
    /// Retorna erro para versão, ID, timestamp, target ou cardinalidade inválidos.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(invalid_session(format!(
                "schema {} não suportado (esperado {SCHEMA_VERSION})",
                self.schema_version
            )));
        }
        let _ = self.id()?;
        if self.revision == 0 {
            return Err(invalid_session("revisão deve ser maior que zero"));
        }
        if !is_utc_rfc3339(&self.created_at) || !is_utc_rfc3339(&self.updated_at) {
            return Err(invalid_session("timestamp fora de UTC RFC3339"));
        }
        if self.targets.is_empty() {
            return Err(invalid_session("a sessão precisa de ao menos um target"));
        }
        let mut names = Vec::with_capacity(self.targets.len());
        for target in &self.targets {
            if target.target.trim().is_empty() || names.contains(&target.target) {
                return Err(invalid_session("targets vazios ou duplicados"));
            }
            match &target.state {
                TargetState::Confirmed { id, url } if *id <= 0 || !is_navigable_url(url) => {
                    return Err(invalid_session("receipt confirmado inválido"));
                }
                TargetState::Pending
                | TargetState::AttemptingOrUncertain { .. }
                | TargetState::Confirmed { .. }
                | TargetState::Failed { .. } => {}
            }
            names.push(target.target.clone());
        }
        if self.target_oids.len() != names.len()
            || names
                .iter()
                .any(|target| !self.target_oids.contains_key(target))
        {
            return Err(invalid_session(
                "fingerprint não cobre exatamente os targets",
            ));
        }
        if self.reviewers.len() != self.targets.len() {
            return Err(invalid_session(
                "reviewers e targets precisam ter o mesmo tamanho",
            ));
        }
        Ok(())
    }

    /// Retorna se nenhum target ainda precisa de uma decisão.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.targets
            .iter()
            .all(|target| target.state.is_confirmed())
    }
}

/// Linha resumida exibida no seletor `--resume`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    /// UUID da sessão.
    pub session_id: String,
    /// Repositório local.
    pub repository: String,
    /// Branch de origem.
    pub source_branch: String,
    /// Data da última alteração.
    pub updated_at: String,
    /// Estados por target.
    pub targets: Vec<(String, &'static str)>,
}

/// Store de uma sessão com lock exclusivo durante o uso.
#[derive(Debug)]
pub struct SessionStore {
    directory: PathBuf,
    session_id: Uuid,
    lock: Option<File>,
    remove_lock_on_drop: Cell<bool>,
    revision: u64,
}

impl SessionStore {
    /// UUID da sessão mantida por este store.
    #[must_use]
    pub const fn session_id(&self) -> Uuid {
        self.session_id
    }

    /// Abre uma sessão já existente e adquire o lock antes de ler.
    ///
    /// # Errors
    ///
    /// Retorna erro quando o lock, arquivo ou schema não pode ser usado.
    pub fn open(paths: &ConfigPaths, id: Uuid) -> Result<(Self, SessionSnapshot)> {
        let mut store = Self::locked(paths, id)?;
        let snapshot = match store.load_latest() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                store.remove_lock_on_drop.set(true);
                return Err(error);
            }
        };
        store.revision = snapshot.revision;
        Ok((store, snapshot))
    }

    /// Cria uma sessão nova, persistindo a primeira revisão.
    ///
    /// # Errors
    ///
    /// Retorna erro quando o snapshot é inválido ou o diretório não pode ser escrito.
    pub fn create(
        paths: &ConfigPaths,
        mut snapshot: SessionSnapshot,
    ) -> Result<(Self, SessionSnapshot)> {
        let id = snapshot.id()?;
        let mut store = Self::locked(paths, id)?;
        snapshot.revision = 0;
        let saved = store.save(snapshot)?;
        Ok((store, saved))
    }

    /// Salva uma nova revisão em arquivo temporário + rename atômico.
    ///
    /// # Errors
    ///
    /// Retorna erro quando o snapshot falha na validação ou a escrita atômica não conclui.
    pub fn save(&mut self, mut snapshot: SessionSnapshot) -> Result<SessionSnapshot> {
        if snapshot.id()? != self.session_id {
            return Err(invalid_session("snapshot pertence a outra sessão"));
        }
        snapshot.revision = self.revision.saturating_add(1);
        snapshot.updated_at = utc_now();
        snapshot.validate()?;

        let final_path = self.snapshot_path(snapshot.revision);
        let temporary_path = self.temporary_path(snapshot.revision);
        let result = (|| -> Result<()> {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary_path)?;
            serde_json::to_writer_pretty(&mut file, &snapshot).map_err(|error| {
                invalid_session(format!("não foi possível serializar: {error}"))
            })?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary_path, &final_path)?;
            sync_directory(&self.directory)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary_path);
            return result.map(|()| snapshot);
        }
        self.revision = snapshot.revision;
        prune_old_revisions(&self.directory, self.session_id, self.revision);
        Ok(snapshot)
    }

    /// Remove todas as revisões após manter o lock.
    ///
    /// # Errors
    ///
    /// Retorna erro quando uma revisão não pode ser removida.
    pub fn discard(mut self) -> Result<()> {
        self.discard_files()?;
        let lock_path = lock_path(&self.directory, self.session_id);
        drop(self.lock.take());
        self.remove_lock_on_drop.set(false);
        let _ = fs::remove_file(lock_path);
        Ok(())
    }

    /// Remove snapshots mantendo o lock até o fim da operação.
    ///
    /// # Errors
    ///
    /// Retorna erro quando o diretório não pode ser lido ou algum snapshot não pode ser removido.
    pub fn discard_files(&self) -> Result<()> {
        let directory = self.directory.clone();
        let id = self.session_id;
        for path in session_artifacts(&directory, id)? {
            fs::remove_file(path)?;
        }
        self.remove_lock_on_drop.set(true);
        Ok(())
    }

    /// Lista apenas sessões incompletas e em ordem de atualização decrescente.
    ///
    /// # Errors
    ///
    /// Retorna erro quando a pasta de sessões não pode ser lida.
    pub fn list(paths: &ConfigPaths) -> Result<Vec<SessionSummary>> {
        let directory = sessions_directory(paths);
        if !directory.exists() {
            return Ok(Vec::new());
        }
        let mut ids = Vec::new();
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Some(id) = parse_snapshot_filename(&name) else {
                continue;
            };
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        let mut summaries = Vec::new();
        for id in ids {
            let Ok((_, snapshot)) = Self::open(paths, id) else {
                continue;
            };
            if !snapshot.is_complete() {
                summaries.push(SessionSummary {
                    session_id: snapshot.session_id,
                    repository: snapshot.repository,
                    source_branch: snapshot.source_branch,
                    updated_at: snapshot.updated_at,
                    targets: snapshot
                        .targets
                        .iter()
                        .map(|target| (target.target.clone(), target.state.label()))
                        .collect(),
                });
            }
        }
        summaries.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.session_id.cmp(&right.session_id))
        });
        Ok(summaries)
    }

    fn locked(paths: &ConfigPaths, id: Uuid) -> Result<Self> {
        let directory = sessions_directory(paths);
        fs::create_dir_all(&directory)?;
        let lock_path = lock_path(&directory, id);
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)?;
        lock.try_lock_exclusive()
            .map_err(|error| invalid_session(format!("sessão já está em uso: {error}")))?;
        Ok(Self {
            directory,
            session_id: id,
            lock: Some(lock),
            remove_lock_on_drop: Cell::new(false),
            revision: 0,
        })
    }

    fn load_latest(&self) -> Result<SessionSnapshot> {
        let mut paths = session_files(&self.directory, self.session_id)?;
        paths.sort_by_key(|path| snapshot_revision(path).unwrap_or(0));
        let path = paths
            .pop()
            .ok_or_else(|| invalid_session("sessão não encontrada"))?;
        let expected_revision =
            snapshot_revision(&path).ok_or_else(|| invalid_session("nome de snapshot inválido"))?;
        let mut contents = String::new();
        File::open(path)?.read_to_string(&mut contents)?;
        let snapshot: SessionSnapshot = serde_json::from_str(&contents)
            .map_err(|error| invalid_session(format!("snapshot inválido: {error}")))?;
        snapshot.validate()?;
        if snapshot.revision != expected_revision {
            return Err(invalid_session(
                "revisão do snapshot não corresponde ao nome do arquivo",
            ));
        }
        Ok(snapshot)
    }

    fn snapshot_path(&self, revision: u64) -> PathBuf {
        self.directory.join(format!(
            "{FILE_PREFIX}{}-{revision}{FILE_SUFFIX}",
            self.session_id
        ))
    }

    fn temporary_path(&self, revision: u64) -> PathBuf {
        self.directory
            .join(format!("{FILE_PREFIX}{}-{revision}.tmp", self.session_id))
    }
}

impl Drop for SessionStore {
    fn drop(&mut self) {
        let remove_lock = self.remove_lock_on_drop.get();
        let lock_path = lock_path(&self.directory, self.session_id);
        drop(self.lock.take());
        if remove_lock {
            let _ = fs::remove_file(lock_path);
        }
    }
}

fn sessions_directory(paths: &ConfigPaths) -> PathBuf {
    paths.directory.join(SESSIONS_DIR)
}

fn lock_path(directory: &Path, id: Uuid) -> PathBuf {
    directory.join(format!("{FILE_PREFIX}{id}{LOCK_SUFFIX}"))
}

fn session_files(directory: &Path, id: Uuid) -> Result<Vec<PathBuf>> {
    let prefix = format!("{FILE_PREFIX}{id}-");
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&prefix) && name.ends_with(FILE_SUFFIX) {
            files.push(path);
        }
    }
    Ok(files)
}

fn session_artifacts(directory: &Path, id: Uuid) -> Result<Vec<PathBuf>> {
    let prefix = format!("{FILE_PREFIX}{id}-");
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_temporary = Path::new(&name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("tmp"));
        if name.starts_with(&prefix) && (name.ends_with(FILE_SUFFIX) || is_temporary) {
            files.push(path);
        }
    }
    Ok(files)
}

fn parse_snapshot_filename(name: &str) -> Option<Uuid> {
    let body = name.strip_prefix(FILE_PREFIX)?.strip_suffix(FILE_SUFFIX)?;
    let id = body.rsplit_once('-')?.0;
    Uuid::parse_str(id)
        .ok()
        .filter(|id| id.get_version_num() == 4)
}

fn snapshot_revision(path: &Path) -> Option<u64> {
    path.file_stem()?
        .to_string_lossy()
        .rsplit_once('-')?
        .1
        .parse()
        .ok()
}

fn prune_old_revisions(directory: &Path, id: Uuid, current_revision: u64) {
    let Ok(paths) = session_files(directory, id) else {
        return;
    };
    for path in paths {
        if snapshot_revision(&path).is_some_and(|revision| revision < current_revision) {
            let _ = fs::remove_file(path);
        }
    }
}

#[allow(clippy::unnecessary_wraps)]
fn sync_directory(directory: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        File::open(directory)?.sync_all()?;
    }
    #[cfg(not(unix))]
    let _ = directory;
    Ok(())
}

fn invalid_session(message: impl Into<String>) -> AppError {
    AppError::Session {
        message: message.into(),
    }
}

fn utc_now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let days = i64::try_from(seconds / 86_400).unwrap_or(i64::MAX);
    let day_seconds = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = day_seconds / 3_600;
    let minute = (day_seconds % 3_600) / 60;
    let second = day_seconds % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

// Algoritmo civil_from_days de Howard Hinnant, adaptado para a biblioteca padrão.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + i64::from(month <= 2);
    (year, month as u32, day as u32)
}

fn is_utc_rfc3339(value: &str) -> bool {
    let bytes = value.as_bytes();
    if !(bytes.len() == 20
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b':'
        && bytes[16] == b':'
        && bytes[19] == b'Z'
        && bytes.iter().enumerate().all(|(index, byte)| {
            matches!(index, 4 | 7 | 10 | 13 | 16 | 19) || byte.is_ascii_digit()
        }))
    {
        return false;
    }
    let number = |start: usize, end: usize| {
        value
            .get(start..end)
            .and_then(|part| part.parse::<u32>().ok())
    };
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        number(0, 4),
        number(5, 7),
        number(8, 10),
        number(11, 13),
        number(14, 16),
        number(17, 19),
    ) else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days_in_month).contains(&day) && hour < 24 && minute < 60 && second < 60
}

fn is_navigable_url(value: &str) -> bool {
    let Some(host_and_path) = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
    else {
        return false;
    };
    !host_and_path.is_empty()
        && !host_and_path.starts_with('/')
        && !value.chars().any(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn paths() -> (tempfile::TempDir, ConfigPaths) {
        let dir = tempdir().expect("tempdir");
        let paths = ConfigPaths {
            directory: dir.path().join("pr-tools"),
            config_file: dir.path().join("pr-tools/config.json"),
            env_file: dir.path().join("pr-tools/.env"),
            template_file: dir.path().join("pr-tools/pr-template.md"),
        };
        (dir, paths)
    }

    fn snapshot() -> SessionSnapshot {
        let fingerprint = GitContextFingerprint {
            repository: "C:\\repo".to_owned(),
            source_branch: "feature/1".to_owned(),
            source_oid: "a".repeat(40),
            target_oids: BTreeMap::from([(String::from("dev"), "b".repeat(40))]),
        };
        SessionSnapshot::new(
            fingerprint.repository.clone(),
            None,
            fingerprint.source_branch.clone(),
            "refs/heads/feature/1".to_owned(),
            &fingerprint,
            "Título".to_owned(),
            "Body".to_owned(),
            "42".to_owned(),
            vec![String::new()],
            vec!["dev".to_owned()],
        )
    }

    #[test]
    fn creates_review_snapshot_before_exit() {
        let (_dir, paths) = paths();
        let initial = snapshot();
        let id = initial.session_id.clone();
        let (store, saved) = SessionStore::create(&paths, initial).expect("create");
        assert_eq!(saved.revision, 1);
        assert!(paths.directory.join("sessions").exists());
        drop(store);
        let id = Uuid::parse_str(&id).expect("uuid");
        let (_store, loaded) = SessionStore::open(&paths, id).expect("load");
        assert_eq!(loaded.title, "Título");
        assert_eq!(loaded.body, "Body");
        assert_eq!(loaded.work_item_id, "42");
        assert_eq!(loaded.source_branch, "feature/1");
        assert_eq!(loaded.source_ref, "refs/heads/feature/1");
        assert_eq!(loaded.source_oid, "a".repeat(40));
        assert_eq!(loaded.target_oids["dev"], "b".repeat(40));
        assert_eq!(loaded.reviewers, vec![String::new()]);
        assert!(matches!(loaded.targets[0].state, TargetState::Pending));
        assert_eq!(
            Uuid::parse_str(&loaded.session_id)
                .unwrap()
                .get_version_num(),
            4
        );
        assert!(is_utc_rfc3339(&loaded.created_at));
        assert!(is_utc_rfc3339(&loaded.updated_at));
    }

    #[test]
    fn lists_incomplete_sessions_newest_first_without_loading() {
        let (_dir, paths) = paths();
        let first = snapshot();
        let second = snapshot();
        let (first_store, _) = SessionStore::create(&paths, first).expect("first");
        drop(first_store);
        let (second_store, _) = SessionStore::create(&paths, second).expect("second");
        drop(second_store);
        let list = SessionStore::list(&paths).expect("list");
        assert_eq!(list.len(), 2);
        assert!(list[0].updated_at >= list[1].updated_at);
        assert!(list.iter().all(|item| {
            !item.session_id.is_empty()
                && !item.repository.is_empty()
                && !item.source_branch.is_empty()
                && is_utc_rfc3339(&item.updated_at)
                && item.targets == vec![("dev".to_owned(), "pending")]
        }));
        if list[0].updated_at == list[1].updated_at {
            assert!(list[0].session_id < list[1].session_id);
        }
    }

    #[test]
    fn writes_schema_v1_uuid_v4_utc_rfc3339_allowlisted_data() {
        let (_dir, paths) = paths();
        let (_store, saved) = SessionStore::create(&paths, snapshot()).expect("create");
        let encoded = serde_json::to_value(saved).expect("json");
        assert_eq!(encoded["schemaVersion"], 1);
        assert_eq!(encoded["createdAt"].as_str().unwrap().len(), 20);
        assert_eq!(
            Uuid::parse_str(encoded["sessionId"].as_str().unwrap())
                .unwrap()
                .get_version_num(),
            4
        );
        assert_eq!(encoded["targets"][0]["state"]["kind"], "pending");
        for state in [
            TargetState::Pending,
            TargetState::AttemptingOrUncertain { message: None },
            TargetState::Confirmed {
                id: 7,
                url: "https://example.test/pr/7".to_owned(),
            },
            TargetState::Failed {
                message: "HTTP 403".to_owned(),
            },
        ] {
            let kind = serde_json::to_value(state).expect("state json")["kind"]
                .as_str()
                .unwrap()
                .to_owned();
            assert!(matches!(
                kind.as_str(),
                "pending" | "attempting_or_uncertain" | "confirmed" | "failed"
            ));
        }
        assert!(encoded.get("azurePat").is_none());

        let mut invalid = snapshot();
        invalid.revision = 1;
        invalid.created_at = "2026-02-30T12:00:00Z".to_owned();
        assert!(invalid.validate().is_err());
        invalid.created_at = "2026-02-28T12:00:00Z".to_owned();
        invalid.updated_at = invalid.created_at.clone();
        invalid.targets[0].state = TargetState::Confirmed {
            id: 0,
            url: String::new(),
        };
        assert!(invalid.validate().is_err());
        invalid.targets[0].state = TargetState::Pending;
        invalid.target_oids.clear();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn serialized_snapshot_contains_no_secrets_or_generation_context() {
        let encoded = serde_json::to_string(&snapshot()).expect("json");
        for forbidden in ["azurePat", "apiKey", "token", "prompt", "diff", "log"] {
            assert!(!encoded.contains(forbidden), "found {forbidden}");
        }
    }

    #[test]
    fn snapshot_revision_is_atomic_and_recovers_previous_file() {
        let (_dir, paths) = paths();
        let (store, saved) = SessionStore::create(&paths, snapshot()).expect("create");
        let files: Vec<_> = fs::read_dir(paths.directory.join("sessions"))
            .expect("read")
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        assert_eq!(files.len(), 1);
        assert_eq!(saved.revision, 1);
        assert!(
            fs::read_dir(paths.directory.join("sessions"))
                .expect("read")
                .filter_map(std::result::Result::ok)
                .all(|entry| entry.path().extension().is_none_or(|ext| ext != "tmp"))
        );
        drop(store);
        let (mut store, mut next) =
            SessionStore::open(&paths, Uuid::parse_str(&saved.session_id).expect("uuid"))
                .expect("reopen");
        next.title = "Título 2".to_owned();
        let saved = store.save(next).expect("second revision");
        assert_eq!(saved.revision, 2);
        let temp = store.temporary_path(3);
        fs::write(&temp, b"interrupted before rename").expect("temporary write");
        drop(store);
        let (_store, recovered) =
            SessionStore::open(&paths, Uuid::parse_str(&saved.session_id).expect("uuid"))
                .expect("recover previous durable revision");
        assert_eq!(recovered.revision, 2);
        assert_eq!(recovered.title, "Título 2");
        assert!(temp.exists());
    }

    #[test]
    fn session_files_stay_below_config_directory() {
        let (_dir, paths) = paths();
        let (_store, _) = SessionStore::create(&paths, snapshot()).expect("create");
        let session_dir = fs::canonicalize(paths.directory.join("sessions")).expect("canonical");
        let config_dir = fs::canonicalize(paths.directory).expect("canonical");
        assert!(session_dir.starts_with(config_dir));
        for entry in fs::read_dir(&session_dir).expect("read session files") {
            let path = fs::canonicalize(entry.expect("entry").path()).expect("canonical file");
            assert!(path.starts_with(&session_dir));
        }
    }

    #[test]
    fn second_session_lock_fails_before_side_effects() {
        let (_dir, paths) = paths();
        let initial = snapshot();
        let id = Uuid::parse_str(&initial.session_id).expect("uuid");
        let (store, _) = SessionStore::create(&paths, initial).expect("create");
        let before = fs::read_dir(paths.directory.join("sessions"))
            .expect("read")
            .map(|entry| entry.expect("entry").path())
            .collect::<Vec<_>>();
        let error = SessionStore::open(&paths, id).expect_err("lock must fail");
        assert!(error.to_string().contains("sessão já está em uso"));
        let after = fs::read_dir(paths.directory.join("sessions"))
            .expect("read")
            .map(|entry| entry.expect("entry").path())
            .collect::<Vec<_>>();
        assert_eq!(before, after);
        drop(store);
    }

    #[test]
    fn discard_removes_all_revisions_and_locked_session_is_not_deletable() {
        let (_dir, paths) = paths();
        let initial = snapshot();
        let id = Uuid::parse_str(&initial.session_id).expect("uuid");
        let (store, _) = SessionStore::create(&paths, initial).expect("create");
        let temporary = store.temporary_path(2);
        fs::write(&temporary, b"interrupted").expect("temporary");
        assert!(SessionStore::open(&paths, id).is_err());
        store.discard().expect("discard");
        assert!(!lock_path(&sessions_directory(&paths), id).exists());
        assert!(!temporary.exists());
        assert!(SessionStore::open(&paths, id).is_err());
    }

    #[test]
    fn rejects_missing_corrupt_and_unknown_schema_without_remote_call() {
        let (_dir, paths) = paths();
        let id = Uuid::new_v4();
        assert!(SessionStore::open(&paths, id).is_err());
        let initial = snapshot();
        let id = Uuid::parse_str(&initial.session_id).expect("uuid");
        let (store, _) = SessionStore::create(&paths, initial).expect("create");
        drop(store);
        let path = fs::read_dir(paths.directory.join("sessions"))
            .expect("read")
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.path())
            .find(|path| path.extension().is_some_and(|ext| ext == "json"))
            .expect("snapshot");
        let valid = fs::read_to_string(&path).expect("valid snapshot");
        fs::write(&path, b"not-json").expect("corrupt");
        assert!(SessionStore::open(&paths, id).is_err());
        fs::write(&path, &valid).expect("restore valid snapshot");
        let mut json: serde_json::Value = serde_json::from_str(&valid).expect("json");
        json["schemaVersion"] = serde_json::json!(2);
        fs::write(&path, serde_json::to_vec(&json).expect("json")).expect("unknown schema");
        assert!(SessionStore::open(&paths, id).is_err());
        fs::write(&path, &valid).expect("restore valid snapshot");
        json["schemaVersion"] = serde_json::json!(1);
        json["unexpectedSecret"] = serde_json::json!("do-not-load");
        fs::write(&path, serde_json::to_vec(&json).expect("json")).expect("unknown field");
        assert!(SessionStore::open(&paths, id).is_err());
    }

    #[test]
    fn schema_rejection_preserves_last_valid_snapshot() {
        let (_dir, paths) = paths();
        let initial = snapshot();
        let id = Uuid::parse_str(&initial.session_id).expect("uuid");
        let (mut store, saved) = SessionStore::create(&paths, initial).expect("create");
        let mut invalid = saved;
        invalid.schema_version = 99;
        assert!(store.save(invalid).is_err());
        drop(store);
        let (_store, loaded) = SessionStore::open(&paths, id).expect("previous snapshot");
        assert_eq!(loaded.revision, 1);

        let initial = snapshot();
        let id = Uuid::parse_str(&initial.session_id).expect("uuid");
        let (store, saved) = SessionStore::create(&paths, initial).expect("second create");
        let path = store.snapshot_path(saved.revision);
        drop(store);
        let mut json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("snapshot")).expect("json");
        json["unknownField"] = serde_json::json!("must reject");
        fs::write(&path, serde_json::to_vec(&json).expect("json")).expect("write unknown field");
        assert!(SessionStore::open(&paths, id).is_err());
    }

    #[test]
    fn persists_pending_then_attempting_before_publish_request() {
        let (_dir, paths) = paths();
        let initial = snapshot();
        let id = Uuid::parse_str(&initial.session_id).expect("uuid");
        let (mut store, saved) = SessionStore::create(&paths, initial).expect("create");
        assert!(matches!(saved.targets[0].state, TargetState::Pending));
        let mut attempting = saved;
        attempting.targets[0].state = TargetState::AttemptingOrUncertain { message: None };
        store.save(attempting).expect("attempting");
        drop(store);
        let (_store, loaded) = SessionStore::open(&paths, id).expect("load");
        assert_eq!(loaded.targets[0].state.label(), "attempting_or_uncertain");
    }

    #[test]
    fn publish_is_blocked_when_attempting_snapshot_cannot_flush() {
        let (_dir, paths) = paths();
        let initial = snapshot();
        let id = Uuid::parse_str(&initial.session_id).expect("uuid");
        let (mut store, saved) = SessionStore::create(&paths, initial).expect("create");
        assert!(matches!(saved.targets[0].state, TargetState::Pending));
        let mut attempting = saved;
        attempting.targets[0].state = TargetState::AttemptingOrUncertain { message: None };
        let temporary = store.temporary_path(2);
        fs::write(&temporary, b"write collision").expect("temporary");
        let error = store.save(attempting).expect_err("durable write must fail");
        assert!(!error.to_string().is_empty());
        drop(store);
        let (_store, loaded) = SessionStore::open(&paths, id).expect("previous snapshot");
        assert_eq!(loaded.targets[0].state.label(), "pending");
    }

    #[test]
    fn successful_create_with_failed_confirmation_write_is_uncertain_on_resume() {
        let (_dir, paths) = paths();
        let initial = snapshot();
        let id = Uuid::parse_str(&initial.session_id).expect("uuid");
        let (mut store, mut saved) = SessionStore::create(&paths, initial).expect("create");
        saved.targets[0].state = TargetState::AttemptingOrUncertain {
            message: Some("receipt não persistida".to_owned()),
        };
        store.save(saved).expect("uncertain");
        let remote_create_calls = std::sync::atomic::AtomicUsize::new(0);
        remote_create_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let temporary = store.temporary_path(3);
        fs::write(&temporary, b"confirmation write collision").expect("temporary");
        let mut confirmed = snapshot();
        confirmed.session_id = id.to_string();
        confirmed.revision = 2;
        confirmed.targets[0].state = TargetState::Confirmed {
            id: 99,
            url: "https://example.test/pr/99".to_owned(),
        };
        assert!(store.save(confirmed).is_err());
        drop(store);
        let (_store, loaded) = SessionStore::open(&paths, id).expect("load");
        assert!(matches!(
            loaded.targets[0].state,
            TargetState::AttemptingOrUncertain { .. }
        ));
        assert_eq!(
            remote_create_calls.load(std::sync::atomic::Ordering::Relaxed),
            1
        );
    }
}
