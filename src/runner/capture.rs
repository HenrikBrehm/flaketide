//! Stream-capture helper: read child stdout/stderr to a bounded buffer,
//! optionally tee to the parent's stdout/stderr.

use tokio::io::{AsyncRead, AsyncReadExt};

const MAX_CAPTURE_BYTES: usize = 8 * 1024 * 1024;

pub async fn collect_stream<R: AsyncRead + Unpin>(
    mut stream: R,
    tee: bool,
    is_stderr: bool,
) -> std::io::Result<Vec<u8>> {
    use std::io::Write as _;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut truncated = false;
    loop {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        if tee {
            if is_stderr {
                let mut err = std::io::stderr().lock();
                let _ = err.write_all(&chunk[..n]);
            } else {
                let mut out = std::io::stdout().lock();
                let _ = out.write_all(&chunk[..n]);
            }
        }
        if buf.len() < MAX_CAPTURE_BYTES {
            let take = (MAX_CAPTURE_BYTES - buf.len()).min(n);
            buf.extend_from_slice(&chunk[..take]);
            if take < n {
                truncated = true;
            }
        } else {
            truncated = true;
        }
    }
    if truncated {
        buf.extend_from_slice(b"\n... [output truncated by flaketide @8 MiB cap] ...\n");
    }
    Ok(buf)
}
