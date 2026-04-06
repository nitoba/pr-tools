package clipboard

import (
	"errors"
	"os/exec"
	"runtime"
	"strings"
)

var ErrUnavailable = errors.New("clipboard: no compatible tool found")

// Write copies text to the system clipboard.
func Write(text string) error {
	switch runtime.GOOS {
	case "darwin":
		return writeMac(text)
	case "linux":
		return writeLinux(text)
	case "windows":
		return writeWindows(text)
	default:
		return ErrUnavailable
	}
}

func writeMac(text string) error {
	cmd := exec.Command("pbcopy")
	cmd.Stdin = strings.NewReader(text)
	return cmd.Run()
}

func writeLinux(text string) error {
	// Try wl-copy first (Wayland)
	if _, err := exec.LookPath("wl-copy"); err == nil {
		cmd := exec.Command("wl-copy")
		cmd.Stdin = strings.NewReader(text)
		if err := cmd.Run(); err == nil {
			return nil
		}
	}
	// Fall back to xclip
	if _, err := exec.LookPath("xclip"); err == nil {
		cmd := exec.Command("xclip", "-selection", "clipboard")
		cmd.Stdin = strings.NewReader(text)
		return cmd.Run()
	}
	// Fall back to xsel
	if _, err := exec.LookPath("xsel"); err == nil {
		cmd := exec.Command("xsel", "--clipboard", "--input")
		cmd.Stdin = strings.NewReader(text)
		return cmd.Run()
	}
	return ErrUnavailable
}

func writeWindows(text string) error {
	cmd := exec.Command("cmd", "/c", "clip")
	cmd.Stdin = strings.NewReader(text)
	return cmd.Run()
}
