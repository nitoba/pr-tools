//! Suspensão da TUI com Ctrl+Z — devolve o terminal ao shell e retoma depois.
//!
//! Em raw mode o Ctrl+Z chega como evento de tecla, não como sinal,
//! então os loops tratam `Ctrl+Z` chamando [`suspend_to_shell`].

use ratatui::DefaultTerminal;

/// Suspende a TUI, devolve o controle ao shell e reconstrói ao retomar.
///
/// Restaura o terminal com `ratatui::restore()`, emite `SIGTSTP` para si
/// e, ao retomar (`SIGCONT`), reconstrói com `ratatui::init()` + `clear()`
/// best-effort. No Windows é no-op (retorna `Ok` sem suspender).
///
/// O chamador deve forçar um redraw total no retorno
/// (`needs_draw`/`dirty = true`), pois o `clear()` best-effort pode
/// não bastar em todos os terminais.
///
/// # Errors
///
/// Atualmente sempre retorna `Ok`; o `Result` existe para propagação
/// com `?` nos loops sem perder contexto.
#[allow(unsafe_code)] // único `unsafe` do binário: `raise(SIGTSTP)` abaixo
pub fn suspend_to_shell(terminal: &mut DefaultTerminal) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        ratatui::restore();
        // SAFETY: `raise(SIGTSTP)` é async-signal-safe e só suspende o processo.
        unsafe {
            libc::raise(libc::SIGTSTP);
        }
        *terminal = ratatui::init();
        let _ = terminal.clear();
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = terminal;
        Ok(())
    }
}
