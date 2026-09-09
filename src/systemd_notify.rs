#[cfg(target_os = "linux")]
pub fn notify_ready() -> std::io::Result<()> {
    use std::env;
    use std::ffi::OsStr;
    use std::os::linux::net::SocketAddrExt as _;
    use std::os::unix::ffi::OsStrExt as _;
    use std::os::unix::net::{SocketAddr, UnixDatagram};
    use std::path::Path;

    let Some(address) = env::var_os("NOTIFY_SOCKET") else {
        return Ok(());
    };
    let address = address.as_os_str().as_bytes();
    let socket = UnixDatagram::unbound()?;
    if let Some(abstract_name) = address.strip_prefix(b"@") {
        let address = SocketAddr::from_abstract_name(abstract_name)?;
        socket.send_to_addr(b"READY=1", &address)?;
    } else {
        socket.send_to(b"READY=1", Path::new(OsStr::from_bytes(address)))?;
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn notify_ready() -> std::io::Result<()> {
    Ok(())
}
