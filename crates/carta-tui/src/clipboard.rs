use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Default)]
pub struct Clipboard {
    internal: String,
}

impl Clipboard {
    pub fn clear_internal(&mut self) {
        self.internal.clear();
    }
    pub fn copy(&mut self, text: String) {
        self.internal = text.clone();
        for (program, args) in [
            ("wl-copy", &[][..]),
            ("xclip", &["-selection", "clipboard"][..]),
            ("pbcopy", &[][..]),
        ] {
            if write_command(program, args, text.as_bytes()) {
                break;
            }
        }
    }
    pub fn paste(&mut self) -> String {
        for (program, args) in [
            ("wl-paste", &["--no-newline"][..]),
            ("xclip", &["-selection", "clipboard", "-o"][..]),
            ("pbpaste", &[][..]),
        ] {
            if let Ok(output) = Command::new(program).args(args).output() {
                if output.status.success() {
                    if let Ok(text) = String::from_utf8(output.stdout) {
                        self.internal = text.clone();
                        return text;
                    }
                }
            }
        }
        self.internal.clone()
    }
}

fn write_command(program: &str, args: &[&str], bytes: &[u8]) -> bool {
    let Ok(mut child) = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    if child
        .stdin
        .take()
        .is_none_or(|mut input| input.write_all(bytes).is_err())
    {
        return false;
    }
    child.wait().is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clearing_internal_clipboard_removes_fallback_content() {
        let mut clipboard = Clipboard {
            internal: "sensitive".to_owned(),
        };
        clipboard.clear_internal();
        assert!(clipboard.internal.is_empty());
    }
}
