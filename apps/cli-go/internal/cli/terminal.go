package cli

import "golang.org/x/term"

// isTerminalFd reports whether the given file descriptor is a terminal.
func isTerminalFd(fd int) bool {
	return term.IsTerminal(fd)
}
