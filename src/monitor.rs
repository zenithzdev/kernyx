use std::io::IsTerminal;
use std::process::Command;

pub fn run() -> anyhow::Result<()> {
    if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
        return Err(anyhow::anyhow!(
            "kernyx monitor requires an interactive terminal (TTY)."
        ));
    }

    // btop can spin at 100% CPU when the terminal reports a zero size.
    // Refuse to start in that case instead of launching a runaway process.
    let (rows, cols) = terminal_size();
    if rows == 0 || cols == 0 {
        return Err(anyhow::anyhow!(
            "terminal size could not be determined; resize the window and retry."
        ));
    }

    let candidates = [
        "/usr/lib/kernyx/kernyx-monitor",
        "target/btop/kernyx-monitor",
        "/tmp/btop/bin/btop",
    ];
    for c in candidates {
        if std::path::Path::new(c).exists() {
            let status = Command::new(c)
                .arg("--config")
                .arg("/etc/kernyx/btop.conf")
                .status()?;
            if !status.success() {
                return Err(anyhow::anyhow!("monitor exited with status {}", status));
            }
            return Ok(());
        }
    }
    Err(anyhow::anyhow!(
        "Bundled monitor binary not found. Reinstall the kernyx package."
    ))
}

fn terminal_size() -> (u16, u16) {
    #[repr(C)]
    struct WinSize {
        ws_row: u16,
        ws_col: u16,
        ws_xpixel: u16,
        ws_ypixel: u16,
    }
    let mut ws: WinSize = unsafe { std::mem::zeroed() };
    const TIOCGWINSZ: libc::c_ulong = 0x5413;
    unsafe {
        if libc::ioctl(0, TIOCGWINSZ, &mut ws) == 0 {
            (ws.ws_row, ws.ws_col)
        } else {
            (0, 0)
        }
    }
}
