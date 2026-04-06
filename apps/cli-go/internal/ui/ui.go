package ui

import (
	"fmt"
	"io"
	"os"
	"sync"
	"time"

	"golang.org/x/term"
)

// Color codes (ANSI)
var (
	Bold        = "\033[1m"
	Dim         = "\033[2m"
	Green       = "\033[0;32m"
	Red         = "\033[0;31m"
	Yellow      = "\033[1;33m"
	Cyan        = "\033[0;36m"
	Orange      = "\033[38;2;193;95;60m"
	OrangeLight = "\033[38;2;224;130;85m"
	OrangeDim   = "\033[38;2;153;75;48m"
	Gray        = "\033[38;5;242m"
	Reset       = "\033[0m"
)

var colorEnabled = true

// Init disables colors if PRT_NO_COLOR or NO_COLOR are set, or if w is not a terminal.
func Init(w io.Writer) {
	if os.Getenv("PRT_NO_COLOR") != "" || os.Getenv("NO_COLOR") != "" {
		disableColors()
		return
	}
	// Check if the writer is an *os.File and if that file is a terminal
	if f, ok := w.(*os.File); ok {
		if !term.IsTerminal(int(f.Fd())) {
			disableColors()
		}
	} else {
		// Not a file — assume not a terminal
		disableColors()
	}
}

func disableColors() {
	colorEnabled = false
	Bold = ""
	Dim = ""
	Green = ""
	Red = ""
	Yellow = ""
	Cyan = ""
	Orange = ""
	OrangeLight = ""
	OrangeDim = ""
	Gray = ""
	Reset = ""
}

// Title prints: ✦ <msg>  in orange/bold to w.
func Title(w io.Writer, msg string) {
	fmt.Fprintf(w, "\n %s%s✦%s %s%s%s\n", Orange, Bold, Reset, OrangeLight, msg, Reset)
}

// TitleDone prints the closing │ └ to w.
func TitleDone(w io.Writer) {
	fmt.Fprintf(w, "  %s│%s └\n", OrangeDim, Reset)
}

// Info prints: │ <msg> (dim) to w.
func Info(w io.Writer, msg string) {
	fmt.Fprintf(w, "  %s│%s %s%s%s\n", OrangeDim, Reset, Dim, msg, Reset)
}

// Warn prints: │ ⚠ <msg> (yellow) to w.
func Warn(w io.Writer, msg string) {
	fmt.Fprintf(w, "  %s│%s %s⚠ %s%s\n", OrangeDim, Reset, Yellow, msg, Reset)
}

// Error prints: │ ✗ <msg> (red) to w.
func Error(w io.Writer, msg string) {
	fmt.Fprintf(w, "  %s│%s %s✗ %s%s\n", OrangeDim, Reset, Red, msg, Reset)
}

// Success prints: │ ✓ <msg> (green) to w.
func Success(w io.Writer, msg string) {
	fmt.Fprintf(w, "  %s│%s %s✓ %s%s\n", OrangeDim, Reset, Green, msg, Reset)
}

// Step prints: │ ● <msg>...  to w (inline, no newline).
// A goroutine blinks ● between bold and dim every 300ms.
// Returns a stop func — call stop(true) for success (prints ✓), stop(false) for failure (prints ✗).
func Step(w io.Writer, msg string) func(ok bool) {
	done := make(chan struct{})
	var once sync.Once

	go func() {
		bold := true
		for {
			select {
			case <-done:
				return
			default:
				if bold {
					fmt.Fprintf(w, "\r  %s│%s %s●%s  %s...", OrangeDim, Reset, Bold, Reset, msg)
				} else {
					fmt.Fprintf(w, "\r  %s│%s %s●%s  %s...", OrangeDim, Reset, Dim, Reset, msg)
				}
				bold = !bold
				time.Sleep(300 * time.Millisecond)
			}
		}
	}()

	stop := func(ok bool) {
		once.Do(func() {
			close(done)
			// Small sleep to let the goroutine stop before we print
			time.Sleep(50 * time.Millisecond)
			// Clear the line
			fmt.Fprintf(w, "\r\033[2K")
			if ok {
				fmt.Fprintf(w, "  %s│%s %s✓%s  %s\n", OrangeDim, Reset, Green, Reset, msg)
			} else {
				fmt.Fprintf(w, "  %s│%s %s✗%s  %s\n", OrangeDim, Reset, Red, Reset, msg)
			}
		})
	}

	return stop
}
