use std::io;

use tokio::io::{AsyncRead, AsyncWrite};

pub trait LocalStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> LocalStream for T {}

pub type Stream = Box<dyn LocalStream>;

#[cfg(unix)]
mod platform {
    use std::{
        os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
        path::{Path, PathBuf},
    };

    use tokio::{fs, net::UnixListener, net::UnixStream};

    use super::{Stream, io};

    pub struct Listener {
        inner: UnixListener,
        _guard: SocketGuard,
    }

    impl Listener {
        pub async fn bind(endpoint: &str) -> io::Result<Self> {
            let path = Path::new(endpoint);
            prepare_socket_path(path).await?;
            let inner = UnixListener::bind(path)?;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
            let metadata = std::fs::symlink_metadata(path)?;
            Ok(Self {
                inner,
                _guard: SocketGuard {
                    path: path.to_owned(),
                    device: metadata.dev(),
                    inode: metadata.ino(),
                },
            })
        }

        pub async fn accept(&mut self) -> io::Result<Stream> {
            let (stream, _) = self.inner.accept().await?;
            Ok(Box::new(stream))
        }
    }

    pub async fn connect(endpoint: &str) -> io::Result<Stream> {
        Ok(Box::new(UnixStream::connect(endpoint).await?))
    }

    async fn prepare_socket_path(path: &Path) -> io::Result<()> {
        match fs::symlink_metadata(path).await {
            Ok(metadata) => {
                if !metadata.file_type().is_socket() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!(
                            "endpoint exists and is not a Unix socket: {}",
                            path.display()
                        ),
                    ));
                }
                match UnixStream::connect(path).await {
                    Ok(_) => {
                        return Err(io::Error::new(
                            io::ErrorKind::AlreadyExists,
                            format!("a daemon is already listening at {}", path.display()),
                        ));
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
                        ) =>
                    {
                        fs::remove_file(path).await?;
                    }
                    Err(error) => return Err(error),
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }

        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            match fs::metadata(parent).await {
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    fs::create_dir_all(parent).await?;
                    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    struct SocketGuard {
        path: PathBuf,
        device: u64,
        inode: u64,
    }

    impl Drop for SocketGuard {
        fn drop(&mut self) {
            let owns_socket = std::fs::symlink_metadata(&self.path).is_ok_and(|metadata| {
                metadata.file_type().is_socket()
                    && metadata.dev() == self.device
                    && metadata.ino() == self.inode
            });
            if owns_socket {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }
}

#[cfg(windows)]
mod platform {
    use tokio::net::windows::named_pipe::{
        ClientOptions, NamedPipeServer, ServerOptions as NamedPipeOptions,
    };

    use super::{Stream, io};

    pub struct Listener {
        endpoint: String,
        next: NamedPipeServer,
    }

    impl Listener {
        pub async fn bind(endpoint: &str) -> io::Result<Self> {
            validate_endpoint(endpoint)?;
            let next = NamedPipeOptions::new()
                .first_pipe_instance(true)
                .reject_remote_clients(true)
                .create(endpoint)?;
            Ok(Self {
                endpoint: endpoint.to_owned(),
                next,
            })
        }

        pub async fn accept(&mut self) -> io::Result<Stream> {
            self.next.connect().await?;
            let next = NamedPipeOptions::new()
                .reject_remote_clients(true)
                .create(&self.endpoint)?;
            let connected = std::mem::replace(&mut self.next, next);
            Ok(Box::new(connected))
        }
    }

    pub async fn connect(endpoint: &str) -> io::Result<Stream> {
        validate_endpoint(endpoint)?;
        Ok(Box::new(ClientOptions::new().open(endpoint)?))
    }

    fn validate_endpoint(endpoint: &str) -> io::Result<()> {
        if endpoint.starts_with(r"\\.\pipe\") {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                r"Windows endpoints must start with \\.\pipe\",
            ))
        }
    }
}

pub use platform::{Listener, connect};
