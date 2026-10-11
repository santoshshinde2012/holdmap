//! Terminal input modes live for exactly one session, including setup errors and unwinding.

use ratatui::crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use ratatui::crossterm::execute;
use std::io::{self, Write};

pub(super) struct InputModes<W: Write> {
    writer: W,
}

impl<W: Write> InputModes<W> {
    pub(super) fn enable(writer: W) -> io::Result<Self> {
        let mut modes = Self { writer };
        // Keep the guard alive before writing: even partial setup must be undone.
        execute!(modes.writer, EnableMouseCapture, EnableBracketedPaste)?;
        Ok(modes)
    }
}

impl<W: Write> Drop for InputModes<W> {
    fn drop(&mut self) {
        // Attempt both independently if a write or flush fails during restoration.
        let _ = execute!(self.writer, DisableBracketedPaste);
        let _ = execute!(self.writer, DisableMouseCapture);
    }
}

/// Declared before initialization and input modes, so those modes are dropped first.
/// Ratatui's own panic hook remains installed and restores raw mode before diagnostics.
pub(super) struct RestoreTerminal;

impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        ratatui::restore();
    }
}

// On Windows these commands can use the console API instead of the injected ANSI writer.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone, Default)]
    struct Capture {
        bytes: Rc<RefCell<Vec<u8>>>,
        fail_flush: Rc<RefCell<bool>>,
    }

    impl Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes.borrow_mut().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            if std::mem::take(&mut *self.fail_flush.borrow_mut()) {
                Err(io::Error::other("injected flush failure"))
            } else {
                Ok(())
            }
        }
    }

    fn restored(capture: &Capture) {
        let bytes = capture.bytes.borrow();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("\x1b[?2004h"), "paste mode enabled");
        assert!(text.contains("\x1b[?1000h"), "mouse mode enabled");
        assert!(text.contains("\x1b[?2004l"), "paste mode restored");
        assert!(text.contains("\x1b[?1000l"), "mouse mode restored");
        assert!(text.find("\x1b[?2004h") < text.find("\x1b[?2004l"));
    }

    #[test]
    fn input_modes_are_paired_on_normal_exit() {
        let capture = Capture::default();
        drop(InputModes::enable(capture.clone()).unwrap());
        restored(&capture);
    }

    #[test]
    fn partial_setup_restores_both_modes_before_returning_the_error() {
        let capture = Capture::default();
        *capture.fail_flush.borrow_mut() = true;
        assert!(InputModes::enable(capture.clone()).is_err());
        restored(&capture);
    }

    #[test]
    fn unwind_and_a_cleanup_error_still_attempt_both_restorations() {
        let capture = Capture::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _modes = InputModes::enable(capture.clone()).unwrap();
            *capture.fail_flush.borrow_mut() = true;
            panic!("injected application panic");
        }));
        assert!(result.is_err());
        restored(&capture);
    }
}
