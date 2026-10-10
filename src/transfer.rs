use std::{fs::File, io::{Read, Write}, path::Path, sync::atomic::{AtomicBool, Ordering}, time::{Duration, Instant}};

#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub received: u64,
    pub total: Option<u64>,
}

pub const CANCELLED: &str = "Cancelled";

pub fn check(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) { Err(CANCELLED.into()) } else { Ok(()) }
}

pub fn download(url: &str, destination: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(Progress)) -> Result<(), String> {
    check(cancel)?;
    let response = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15)).timeout_read(Duration::from_secs(5))
        .timeout_write(Duration::from_secs(15)).redirects(8).build()
        .get(url).set("User-Agent", "CraftLauncher").call()
        .map_err(|error| if check(cancel).is_err() { CANCELLED.into() } else { format!("Download failed: {error}") })?;
    check(cancel)?;
    let total = response.header("Content-Length").and_then(|value| value.parse::<u64>().ok());
    let result = copy(response.into_reader(), destination, total, cancel, progress);
    if result.is_err() { let _ = std::fs::remove_file(destination); }
    result
}

fn copy(mut reader: impl Read, destination: &Path, total: Option<u64>, cancel: &AtomicBool, progress: &mut dyn FnMut(Progress)) -> Result<(), String> {
    let mut file = File::create(destination).map_err(|error| format!("Couldn't save download: {error}"))?;
    let mut buffer = [0_u8; 64 * 1024];
    let mut received = 0;
    let mut last = Instant::now();
    progress(Progress { received, total });
    loop {
        check(cancel)?;
        let count = reader.read(&mut buffer).map_err(|error| if check(cancel).is_err() { CANCELLED.into() } else { format!("Connection interrupted: {error}") })?;
        if count == 0 { break; }
        check(cancel)?;
        file.write_all(&buffer[..count]).map_err(|error| format!("Couldn't save download. Check free disk space: {error}"))?;
        received += count as u64;
        if last.elapsed() >= Duration::from_millis(80) {
            progress(Progress { received, total });
            last = Instant::now();
        }
    }
    check(cancel)?;
    if total.is_some_and(|expected| expected != received) { return Err("Download was incomplete. Try again.".into()); }
    file.sync_all().map_err(|error| format!("Couldn't finish saving download: {error}"))?;
    progress(Progress { received, total });
    Ok(())
}

pub fn size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 { format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0)) }
    else if bytes >= 1024 * 1024 { format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)) }
    else { format!("{:.0} KB", bytes as f64 / 1024.0) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn truncated_download_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        assert!(copy(&b"short"[..], &dir.path().join("file"), Some(100), &AtomicBool::new(false), &mut |_| {}).is_err());
    }
    #[test]
    fn cancellation_interrupts_before_file_write() {
        let dir = tempfile::tempdir().unwrap();
        let cancelled = AtomicBool::new(true);
        assert_eq!(copy(&b"data"[..], &dir.path().join("file"), Some(4), &cancelled, &mut |_| {}).unwrap_err(), CANCELLED);
    }
    #[test]
    fn unknown_size_download_reports_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let mut final_progress = Progress::default();
        copy(&b"data"[..], &dir.path().join("file"), None, &AtomicBool::new(false), &mut |p| final_progress = p).unwrap();
        assert_eq!(final_progress.received, 4);
        assert_eq!(final_progress.total, None);
    }

    #[test]
    fn cancel_during_read_stops_before_writing_chunk() {
        struct CancellingReader<'a>(&'a AtomicBool);
        impl Read for CancellingReader<'_> {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                buffer[0] = 42; self.0.store(true, Ordering::Relaxed); Ok(1)
            }
        }
        let dir = tempfile::tempdir().unwrap(); let cancel = AtomicBool::new(false); let path = dir.path().join("partial");
        assert_eq!(copy(CancellingReader(&cancel), &path, None, &cancel, &mut |_| {}).unwrap_err(), CANCELLED);
        assert_eq!(std::fs::metadata(path).unwrap().len(), 0);
    }

    #[test]
    fn broken_connection_can_be_retried_with_fresh_download() {
        use std::net::TcpListener;
        let server = TcpListener::bind("127.0.0.1:0").unwrap(); let url = format!("http://{}/file", server.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            for body in ["ab", "abcd"] {
                let (mut stream, _) = server.accept().unwrap(); let mut request = [0; 2048]; let _ = stream.read(&mut request);
                let response = format!("HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\n{body}");
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("file"); let cancel = AtomicBool::new(false);
        assert!(download(&url, &path, &cancel, &mut |_| {}).is_err()); assert!(!path.exists());
        download(&url, &path, &cancel, &mut |_| {}).unwrap(); assert_eq!(std::fs::read(path).unwrap(), b"abcd");
        worker.join().unwrap();
    }
}
