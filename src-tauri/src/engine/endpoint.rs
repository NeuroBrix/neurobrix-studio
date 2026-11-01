//! Where Studio's engine instance lives, and how to reach a daemon on it.
//!
//! Studio never recomputes an endpoint: the engine reports where this
//! instance's daemon listens (`info --json` run with NBX_SOCKET_PATH set),
//! and Studio dials what the record says. A named Windows instance has no
//! socket file — the daemon binds a port the OS hands it, records the
//! port in `<stem>.port`, and every client of that instance reads it
//! there (protocol.py `Instance`). Instances never collide, which is why
//! Studio names its own under its application cache directory instead of
//! sharing the user's default instance.

use std::net::IpAddr;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The env key that names an engine instance on every platform
/// (protocol.py `Instance`).
pub const INSTANCE_ENV_KEY: &str = "NBX_SOCKET_PATH";

/// Studio's instance. On Unix the value IS the socket path; on Windows the
/// engine derives the pid/log/port files from its stem. One value, set
/// identically for the owned child, Studio's connections, and the env'd
/// `info --json` probe that reports the endpoint.
#[derive(Debug, Clone)]
pub struct StudioInstance {
    socket_path: PathBuf,
}

impl StudioInstance {
    /// Studio names its instance under its own application cache
    /// directory, so its daemons never collide with a user's terminal
    /// daemon on the default instance.
    pub fn under(cache_dir: &Path) -> Self {
        Self {
            socket_path: cache_dir.join("engine").join("daemon.sock"),
        }
    }

    /// The env pairs every engine process of this instance must carry.
    pub fn env(&self) -> Vec<(String, String)> {
        vec![(
            INSTANCE_ENV_KEY.to_owned(),
            self.socket_path.to_string_lossy().into_owned(),
        )]
    }

    /// The engine binds where Studio pointed it, so the directory must
    /// exist before the daemon starts — the engine only creates its own
    /// default daemon directory.
    pub fn ensure_directory(&self) -> std::io::Result<()> {
        let parent = self
            .socket_path
            .parent()
            .expect("an instance path always has a parent");
        std::fs::create_dir_all(parent)
    }

    /// The files this instance's daemon leaves behind: the socket (Unix),
    /// the pid, and a named Windows instance's port file — mirroring the
    /// engine's `Instance.instance_files()` rule, where the stem's
    /// extensions become `.pid` and `.port`. The log persists by the
    /// engine's own design and is not Studio's to clean.
    pub fn instance_files(&self) -> Vec<PathBuf> {
        vec![
            self.socket_path.clone(),
            self.socket_path.with_extension("pid"),
            self.socket_path.with_extension("port"),
        ]
    }
}

/// The endpoint record exactly as the engine reports it (protocol.py
/// `Instance.endpoint()`): tagged by `kind`, never assembled by Studio.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EndpointRecord {
    Unix {
        path: PathBuf,
    },
    Tcp {
        address: IpAddr,
        /// `None` while no daemon runs on a named Windows instance.
        #[serde(default)]
        port: Option<u16>,
        #[serde(default)]
        port_file: Option<PathBuf>,
    },
}

/// What Studio actually dials, resolved from the record — nothing else.
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectAddress {
    Unix(PathBuf),
    Tcp { host: IpAddr, port: u16 },
}

/// Every way endpoint resolution can fail. "No daemon" is a normal state,
/// not an error.
#[derive(Debug, PartialEq)]
pub enum ResolveError {
    /// Nothing names a port: no daemon ever ran on this instance.
    NoDaemon,
    /// The port file exists but names no usable port — reported, never
    /// guessed around.
    UnusablePortFile(String),
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::NoDaemon => {
                write!(f, "no daemon is running on this instance")
            }
            ResolveError::UnusablePortFile(path) => {
                write!(f, "the instance's port file ({path}) names no usable port")
            }
        }
    }
}

impl std::error::Error for ResolveError {}

impl EndpointRecord {
    /// Resolve to a dialable address, mirroring the engine's
    /// `Instance.connect_address()`: refused by name, never a guess at a
    /// port.
    pub fn resolve(&self) -> Result<ConnectAddress, ResolveError> {
        match self {
            EndpointRecord::Unix { path } => Ok(ConnectAddress::Unix(path.clone())),
            EndpointRecord::Tcp {
                address,
                port,
                port_file,
            } => {
                // A live daemon's record carries the port directly; a down
                // instance reports null and the port file holds the port
                // the OS handed the daemon at bind.
                let port = match port {
                    Some(port) => *port,
                    None => {
                        let Some(file) = port_file else {
                            return Err(ResolveError::NoDaemon);
                        };
                        read_port_file(file)?
                    }
                };
                Ok(ConnectAddress::Tcp {
                    host: *address,
                    port,
                })
            }
        }
    }
}

fn read_port_file(file: &Path) -> Result<u16, ResolveError> {
    // A missing port file is the engine's "no daemon for this instance":
    // a normal stopped state, refused by name.
    if !file.exists() {
        return Err(ResolveError::NoDaemon);
    }
    let unusable = || ResolveError::UnusablePortFile(file.to_string_lossy().into_owned());
    let text = std::fs::read_to_string(file).map_err(|_| unusable())?;
    text.trim().parse::<u16>().map_err(|_| unusable())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The engine's own instance tests
    /// (tests/unit/serving/test_each_windows_instance_has_its_own_port.py)
    /// pin these shapes on its side; these tests pin Studio's reading of
    /// the same records.

    #[test]
    fn a_unix_endpoint_resolves_to_its_recorded_path() {
        let record: EndpointRecord = serde_json::from_value(json!({
            "kind": "unix",
            "path": "/home/user/.neurobrix/daemon.sock"
        }))
        .expect("the engine's Unix shape parses");
        assert_eq!(
            record.resolve(),
            Ok(ConnectAddress::Unix(PathBuf::from(
                "/home/user/.neurobrix/daemon.sock"
            )))
        );
    }

    #[test]
    fn the_default_tcp_endpoint_carries_its_fixed_port() {
        // The default Windows instance keeps the fixed port, no port file.
        let record: EndpointRecord = serde_json::from_value(json!({
            "kind": "tcp",
            "address": "127.0.0.1",
            "port": 19384
        }))
        .expect("the engine's fixed-port shape parses");
        assert_eq!(
            record.resolve(),
            Ok(ConnectAddress::Tcp {
                host: "127.0.0.1".parse().expect("a valid address"),
                port: 19384
            })
        );
    }

    #[test]
    fn a_named_tcp_instance_reads_its_port_from_the_port_file() {
        let file = temp_port_file("live", "41235\n");
        let record: EndpointRecord = serde_json::from_value(json!({
            "kind": "tcp",
            "address": "127.0.0.1",
            "port": null,
            "port_file": file.to_string_lossy()
        }))
        .expect("the engine's named-instance shape parses");
        assert_eq!(
            record.resolve(),
            Ok(ConnectAddress::Tcp {
                host: "127.0.0.1".parse().expect("a valid address"),
                port: 41235
            })
        );
    }

    #[test]
    fn a_missing_port_file_means_no_daemon_not_a_failure() {
        // The engine refuses a named instance without a daemon by name;
        // Studio reads that the same way — a normal stopped state.
        let absent = std::env::temp_dir()
            .join("nbx-studio-endpoint-tests")
            .join("never-created.port");
        let record: EndpointRecord = serde_json::from_value(json!({
            "kind": "tcp",
            "address": "127.0.0.1",
            "port": null,
            "port_file": absent
        }))
        .expect("the engine's named-instance shape parses");
        assert_eq!(record.resolve(), Err(ResolveError::NoDaemon));
    }

    #[test]
    fn a_port_file_that_names_no_port_is_refused_by_name() {
        let file = temp_port_file("garbage", "not-a-port\n");
        let record: EndpointRecord = serde_json::from_value(json!({
            "kind": "tcp",
            "address": "127.0.0.1",
            "port": null,
            "port_file": file.to_string_lossy()
        }))
        .expect("the engine's named-instance shape parses");
        match record.resolve() {
            Err(ResolveError::UnusablePortFile(named)) => assert!(named.ends_with(".port")),
            other => panic!("expected an unusable port file refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_tcp_record_without_a_port_or_a_port_file_names_no_daemon() {
        // The engine never emits this shape; if it ever did, resolving it
        // to "no daemon" is honest — there is nothing to dial.
        let record: EndpointRecord = serde_json::from_value(json!({
            "kind": "tcp",
            "address": "127.0.0.1",
            "port": null
        }))
        .expect("parses");
        assert_eq!(record.resolve(), Err(ResolveError::NoDaemon));
    }

    #[test]
    fn the_instance_names_itself_under_the_application_cache_dir() {
        let instance = StudioInstance::under(Path::new("/data"));
        let env = instance.env();
        assert_eq!(env.len(), 1);
        assert_eq!(env[0].0, INSTANCE_ENV_KEY);
        // On Unix the value is the socket path itself; on Windows the
        // engine derives the pid/log/port files from the same stem.
        let socket_path = PathBuf::from(&env[0].1);
        assert!(socket_path.ends_with(Path::new("engine").join("daemon.sock")));
    }

    #[test]
    fn the_instance_directory_exists_before_the_engine_binds_there() {
        // The engine only creates its own default daemon directory; the
        // directory Studio pointed the instance at is Studio's to make.
        let dir = std::env::temp_dir()
            .join("nbx-studio-endpoint-tests")
            .join("instance-dir");
        let instance = StudioInstance::under(&dir);
        instance.ensure_directory().expect("directory created");
        assert!(dir.join("engine").exists());
    }

    #[test]
    fn the_instance_names_its_daemons_files_by_the_engine_rule() {
        // The engine derives the pid and port files from the socket stem
        // (`Instance.instance_files()`); Studio reads the same rule to
        // confirm cleanup after a stop.
        let dir = Path::new("/data");
        let instance = StudioInstance::under(dir);
        let socket = dir.join("engine").join("daemon.sock");
        assert_eq!(
            instance.instance_files(),
            vec![
                socket.clone(),
                socket.with_extension("pid"),
                socket.with_extension("port"),
            ]
        );
    }

    fn temp_port_file(name: &str, contents: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("nbx-studio-endpoint-tests");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let file = dir.join(format!("{}-{}.port", name, std::process::id()));
        std::fs::write(&file, contents).expect("write the port file");
        file
    }
}
