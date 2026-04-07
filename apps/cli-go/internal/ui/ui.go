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

type session struct {
	mu              sync.Mutex
	interactive     bool
	colorEnabled    bool
	palette         colorSnapshot
	titleActive     bool
	titleMsg        string
	titleLinesBelow int
	stepActive      bool
	stepID          uint64
	stepMsg         string
	stepFrame       int
	nextStepID      uint64
}

type colorSnapshot struct {
	Bold        string
	Dim         string
	Green       string
	Red         string
	Yellow      string
	Cyan        string
	Orange      string
	OrangeLight string
	OrangeDim   string
	Gray        string
	Reset       string
}

var current = &session{}
var defaultColors = snapshotColorsForTest()

// Init disables colors if PRT_NO_COLOR or NO_COLOR are set, or if w is not a terminal.
func Init(w io.Writer) {
	interactive := false
	if f, ok := w.(*os.File); ok {
		interactive = term.IsTerminal(int(f.Fd()))
	}

	colorEnabled := interactive && os.Getenv("PRT_NO_COLOR") == "" && os.Getenv("NO_COLOR") == ""
	palette := defaultColors
	if !colorEnabled {
		palette = colorSnapshot{}
	}

	current.mu.Lock()
	current.interactive = interactive
	current.colorEnabled = colorEnabled
	current.palette = palette
	current.titleActive = false
	current.titleMsg = ""
	current.titleLinesBelow = 0
	current.stepActive = false
	current.stepID = 0
	current.stepMsg = ""
	current.stepFrame = 0
	current.nextStepID = 0
	current.mu.Unlock()

	setGlobalColors(palette)
}

func setGlobalColors(s colorSnapshot) {
	Bold = s.Bold
	Dim = s.Dim
	Green = s.Green
	Red = s.Red
	Yellow = s.Yellow
	Cyan = s.Cyan
	Orange = s.Orange
	OrangeLight = s.OrangeLight
	OrangeDim = s.OrangeDim
	Gray = s.Gray
	Reset = s.Reset
}

func snapshotColorsUnlocked() colorSnapshot {
	return colorSnapshot{
		Bold:        Bold,
		Dim:         Dim,
		Green:       Green,
		Red:         Red,
		Yellow:      Yellow,
		Cyan:        Cyan,
		Orange:      Orange,
		OrangeLight: OrangeLight,
		OrangeDim:   OrangeDim,
		Gray:        Gray,
		Reset:       Reset,
	}
}

func snapshotColorsForTest() colorSnapshot {
	current.mu.Lock()
	defer current.mu.Unlock()

	return snapshotColorsUnlocked()
}

func restoreColorsForTest(s colorSnapshot) {
	current.mu.Lock()
	defer current.mu.Unlock()

	current.palette = s
	setGlobalColors(s)
}

func resetForTest(interactive bool) {
	current = &session{interactive: interactive, colorEnabled: true, palette: defaultColors}
	setGlobalColors(defaultColors)
}

func animationEnabled() bool {
	return current.interactive && current.colorEnabled
}

func writeTitleLineLocked(w io.Writer, content string) {
	p := current.palette
	if current.stepActive && animationEnabled() {
		_, _ = io.WriteString(w, "\r\033[2K")
		_, _ = fmt.Fprintf(w, "  %s│%s %s\n", p.OrangeDim, p.Reset, content)
		current.titleLinesBelow++
		_, _ = io.WriteString(w, renderTick(current.stepFrame, current.titleLinesBelow+1, current.titleMsg, current.stepMsg, p))
		return
	}

	_, _ = fmt.Fprintf(w, "  %s│%s %s\n", p.OrangeDim, p.Reset, content)
	current.titleLinesBelow++
}

// Title prints a ✦ <msg> header to w.
func Title(w io.Writer, msg string) {
	current.mu.Lock()
	defer current.mu.Unlock()

	current.titleActive = true
	current.titleMsg = msg
	current.titleLinesBelow = 0

	p := current.palette
	_, _ = fmt.Fprintf(w, " %s%s✦%s %s%s%s\n", p.Orange, p.Bold, p.Reset, p.OrangeDim, msg, p.Reset)
}

// TitleDone only resets title state.
func TitleDone(io.Writer) {
	current.mu.Lock()
	defer current.mu.Unlock()

	current.titleActive = false
	current.titleMsg = ""
	current.titleLinesBelow = 0
}

// Info prints a status line to w.
func Info(w io.Writer, msg string) {
	current.mu.Lock()
	defer current.mu.Unlock()

	if current.titleActive {
		p := current.palette
		writeTitleLineLocked(w, fmt.Sprintf("%s%s%s", p.Dim, msg, p.Reset))
		return
	}

	p := current.palette
	_, _ = fmt.Fprintf(w, "  %s%s%s\n", p.Dim, msg, p.Reset)
}

// Warn prints a warning line to w.
func Warn(w io.Writer, msg string) {
	current.mu.Lock()
	defer current.mu.Unlock()

	if current.titleActive {
		p := current.palette
		writeTitleLineLocked(w, fmt.Sprintf("%s⚠ %s%s", p.Yellow, msg, p.Reset))
		return
	}

	p := current.palette
	_, _ = fmt.Fprintf(w, "  %s⚠ %s%s\n", p.Yellow, msg, p.Reset)
}

// Error prints an error line to w.
func Error(w io.Writer, msg string) {
	current.mu.Lock()
	defer current.mu.Unlock()

	if current.titleActive {
		p := current.palette
		writeTitleLineLocked(w, fmt.Sprintf("%s✗ %s%s", p.Red, msg, p.Reset))
		return
	}

	p := current.palette
	_, _ = fmt.Fprintf(w, "  %s✗ %s%s\n", p.Red, msg, p.Reset)
}

// Success prints a success line to w.
func Success(w io.Writer, msg string) {
	current.mu.Lock()
	defer current.mu.Unlock()

	if current.titleActive {
		p := current.palette
		writeTitleLineLocked(w, fmt.Sprintf("%s✓ %s%s", p.Green, msg, p.Reset))
		return
	}

	p := current.palette
	_, _ = fmt.Fprintf(w, "  %s✓ %s%s\n", p.Green, msg, p.Reset)
}

func stepWithResult(w io.Writer, msg string) func(ok bool, finalMsg string) {
	done := make(chan struct{})
	var once sync.Once

	current.mu.Lock()
	current.nextStepID++
	stepID := current.nextStepID
	current.stepActive = true
	current.stepID = stepID
	current.stepMsg = msg
	current.stepFrame = 0
	titleActive := current.titleActive
	titleMsg := current.titleMsg
	titleDistance := current.titleLinesBelow + 1
	animate := animationEnabled()
	p := current.palette
	if animate {
		if titleActive {
			_, _ = io.WriteString(w, renderTick(0, titleDistance, titleMsg, msg, p))
		} else {
			_, _ = io.WriteString(w, renderTick(0, 0, "", msg, p))
		}
	}
	current.mu.Unlock()

	if animate {
		go func() {
			ticker := time.NewTicker(110 * time.Millisecond)
			defer ticker.Stop()

			frame := 1
			for {
				select {
				case <-done:
					return
				case <-ticker.C:
					current.mu.Lock()
					if !current.stepActive || current.stepID != stepID {
						current.mu.Unlock()
						return
					}

					titleActive := current.titleActive
					titleMsg := current.titleMsg
					titleDistance := current.titleLinesBelow + 1
					p := current.palette
					current.stepFrame = frame
					if titleActive {
						_, _ = io.WriteString(w, renderTick(frame, titleDistance, titleMsg, msg, p))
					} else {
						_, _ = io.WriteString(w, renderTick(frame, 0, "", msg, p))
					}
					current.mu.Unlock()
					frame++
				}
			}
		}()
	}

	return func(ok bool, finalMsg string) {
		once.Do(func() {
			close(done)

			current.mu.Lock()
			defer current.mu.Unlock()

			titleActive := current.titleActive
			p := current.palette
			if current.stepActive && current.stepID == stepID {
				current.stepActive = false
				current.stepID = 0
				current.stepMsg = ""
				current.stepFrame = 0
			}

			if animate {
				_, _ = io.WriteString(w, "\r\033[2K")
			}

			if finalMsg == "" {
				finalMsg = msg
			}

			if titleActive {
				if ok {
					_, _ = fmt.Fprintf(w, "  %s│%s %s✓%s %s\n", p.OrangeDim, p.Reset, p.Green, p.Reset, finalMsg)
				} else {
					_, _ = fmt.Fprintf(w, "  %s│%s %s✗%s %s\n", p.OrangeDim, p.Reset, p.Red, p.Reset, finalMsg)
				}
				current.titleLinesBelow++
				return
			}

			if ok {
				_, _ = fmt.Fprintf(w, " %s✓%s %s\n", p.Green, p.Reset, finalMsg)
			} else {
				_, _ = fmt.Fprintf(w, " %s✗%s %s\n", p.Red, p.Reset, finalMsg)
			}
		})
	}
}

// Step starts a progress row and returns a stop function.
func Step(w io.Writer, msg string) func(ok bool) {
	stop := stepWithResult(w, msg)
	return func(ok bool) {
		stop(ok, msg)
	}
}

// StepMessage starts a progress row and allows completion with a different label.
func StepMessage(w io.Writer, msg string) func(ok bool, finalMsg string) {
	return stepWithResult(w, msg)
}

func renderTick(frame int, titleDistance int, titleMsg string, stepMsg string, p colorSnapshot) string {
	titleFrames := []struct {
		symbol string
		text   string
	}{
		{symbol: fmt.Sprintf("%s%s✦%s", p.Orange, p.Bold, p.Reset), text: p.OrangeLight},
		{symbol: fmt.Sprintf("%s%s✧%s", p.OrangeDim, p.Dim, p.Reset), text: p.OrangeDim},
		{symbol: fmt.Sprintf("%s%s✦%s", p.Orange, p.Bold, p.Reset), text: p.OrangeLight},
		{symbol: fmt.Sprintf("%s%s·%s", p.OrangeDim, p.Dim, p.Reset), text: p.OrangeDim},
	}

	stepStyle := p.Dim
	if frame%2 == 0 {
		stepStyle = p.Bold
	}

	if titleDistance <= 0 {
		return fmt.Sprintf("\r\033[2K %s●%s  %s...", stepStyle, p.Reset, stepMsg)
	}

	titleFrame := titleFrames[frame%len(titleFrames)]
	return fmt.Sprintf(
		"\r\033[2K\033[s\033[%dA\r\033[2K %s %s%s%s\n\033[u\r\033[2K  %s│%s %s●%s  %s...",
		titleDistance,
		titleFrame.symbol,
		titleFrame.text,
		titleMsg,
		p.Reset,
		p.OrangeDim,
		p.Reset,
		stepStyle,
		p.Reset,
		stepMsg,
	)
}
