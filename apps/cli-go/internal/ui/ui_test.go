package ui

import (
	"bytes"
	"strings"
	"testing"

	"github.com/stretchr/testify/require"
)

func usePlainColorsForTest(t *testing.T, interactive bool) {
	t.Helper()

	resetForTest(interactive)
	snapshot := snapshotColorsForTest()
	restoreColorsForTest(colorSnapshot{})
	t.Cleanup(func() {
		restoreColorsForTest(snapshot)
	})
}

func TestTitleDoesNotEmitLeadingBlankLine(t *testing.T) {
	usePlainColorsForTest(t, false)

	var buf bytes.Buffer
	Title(&buf, "Gerando descrição do PR...")

	out := buf.String()
	require.NotEmpty(t, out)
	require.NotEqual(t, '\n', rune(out[0]))
	require.Contains(t, out, "✦ Gerando descrição do PR...")
}

func TestTitleDoneDoesNotPrintClosingRow(t *testing.T) {
	usePlainColorsForTest(t, false)

	var buf bytes.Buffer
	Title(&buf, "Gerando descrição do PR...")
	before := buf.String()
	TitleDone(&buf)

	require.Equal(t, before, buf.String())
	require.False(t, current.titleActive)
	require.Equal(t, 0, current.titleLinesBelow)
}

func TestInfoWarnErrorSuccessUseTitleTree(t *testing.T) {
	usePlainColorsForTest(t, false)

	var buf bytes.Buffer
	Title(&buf, "Gerando descrição do PR...")
	Info(&buf, "Contexto git coletado")
	Warn(&buf, "Diff truncado")
	Error(&buf, "Todos os providers falharam")
	Success(&buf, "Descrição gerada")

	out := buf.String()
	require.Contains(t, out, "│ Contexto git coletado")
	require.Contains(t, out, "│ ⚠ Diff truncado")
	require.Contains(t, out, "│ ✗ Todos os providers falharam")
	require.Contains(t, out, "│ ✓ Descrição gerada")
	require.Equal(t, 4, current.titleLinesBelow)
}

func TestStepWithActiveTitleReplacesSpinnerWithTreeSuccess(t *testing.T) {
	usePlainColorsForTest(t, false)

	var buf bytes.Buffer
	Title(&buf, "Gerando descrição do PR...")
	stop := Step(&buf, "Validando dependencias")
	stop(true)

	require.Contains(t, buf.String(), "│ ✓ Validando dependencias")
	require.Equal(t, 1, current.titleLinesBelow)
}

func TestStepMessageWithActiveTitleUsesCompletionLabel(t *testing.T) {
	usePlainColorsForTest(t, false)

	var buf bytes.Buffer
	Title(&buf, "Gerando descrição do PR...")
	stop := StepMessage(&buf, "Validando dependencias")
	stop(true, "Dependencias validadas")

	out := buf.String()
	require.Contains(t, out, "│ ✓ Dependencias validadas")
	require.NotContains(t, out, "│ ✓ Validando dependencias")
}

func TestStepMessageWithoutTitleUsesCompletionLabel(t *testing.T) {
	usePlainColorsForTest(t, false)

	var buf bytes.Buffer
	stop := StepMessage(&buf, "Criando PR → dev")
	stop(false, "Falha ao criar PR → dev")

	out := buf.String()
	require.Contains(t, out, "✗ Falha ao criar PR → dev")
	require.NotContains(t, out, "✗ Criando PR → dev")
}

func TestStepWithActiveTitleReplacesSpinnerWithTreeFailure(t *testing.T) {
	usePlainColorsForTest(t, false)

	var buf bytes.Buffer
	Title(&buf, "Gerando descrição do PR...")
	stop := Step(&buf, "Validando API keys")
	stop(false)

	require.Contains(t, buf.String(), "│ ✗ Validando API keys")
	require.Equal(t, 1, current.titleLinesBelow)
}

func TestStepWithoutTitleUsesStandaloneLayoutSuccess(t *testing.T) {
	usePlainColorsForTest(t, false)

	var buf bytes.Buffer
	stop := Step(&buf, "Criando PR → dev")
	stop(true)

	out := buf.String()
	require.Contains(t, out, "✓ Criando PR → dev")
	require.NotContains(t, out, "│")
}

func TestStepWithoutTitleUsesStandaloneLayoutFailure(t *testing.T) {
	usePlainColorsForTest(t, false)

	var buf bytes.Buffer
	stop := Step(&buf, "Criando PR → dev")
	stop(false)

	out := buf.String()
	require.Contains(t, out, "✗ Criando PR → dev")
	require.NotContains(t, out, "│")
}

func TestNonInteractiveStepUsesStaticOutput(t *testing.T) {
	usePlainColorsForTest(t, false)

	var buf bytes.Buffer
	Title(&buf, "Gerando card de teste...")
	stop := Step(&buf, "Buscando exemplos de test case")
	stop(true)

	out := buf.String()
	require.NotContains(t, out, "\r")
	require.Contains(t, out, "│ ✓ Buscando exemplos de test case")
}

func TestInfoWithActiveInteractiveStepRendersSeparateRow(t *testing.T) {
	usePlainColorsForTest(t, true)

	var buf bytes.Buffer
	Title(&buf, "Gerando descrição do PR...")
	stop := Step(&buf, "Validando dependencias")
	Info(&buf, "Contexto git coletado")
	stop(true)

	out := buf.String()
	require.Contains(t, out, "\r\033[2K  │ Contexto git coletado\n")
	require.Contains(t, out, "\n\r\033[2K\033[s\033[2A")
	require.NotContains(t, out, "Validando dependencias...  │ Contexto git coletado")
	require.Contains(t, out, "│ ✓ Validando dependencias")
	require.Equal(t, 2, current.titleLinesBelow)
}

func TestStoppingFirstDuplicateMessageStepKeepsSecondStepActive(t *testing.T) {
	usePlainColorsForTest(t, true)

	var buf bytes.Buffer
	Title(&buf, "Gerando descrição do PR...")

	first := Step(&buf, "Validando dependencias")
	firstID := current.stepID
	second := Step(&buf, "Validando dependencias")
	secondID := current.stepID

	require.NotZero(t, firstID)
	require.NotZero(t, secondID)
	require.NotEqual(t, firstID, secondID)

	first(true)
	require.True(t, current.stepActive)
	require.Equal(t, secondID, current.stepID)

	Info(&buf, "Contexto git coletado")
	second(true)

	out := buf.String()
	require.Contains(t, out, "│ Contexto git coletado\n\r\033[2K\033[s")
	require.Equal(t, 2, strings.Count(out, "│ ✓ Validando dependencias"))
	require.Contains(t, out, "│ Contexto git coletado")
}

func TestRenderTickUsesBashSparkleFramesAndLineOffsets(t *testing.T) {
	p := colorSnapshot{
		Bold:        "<bold>",
		Dim:         "<dim>",
		Orange:      "<orange>",
		OrangeLight: "<orange-light>",
		OrangeDim:   "<orange-dim>",
		Reset:       "<reset>",
	}

	frame0 := renderTick(0, 3, "Gerando descrição do PR...", "Validando API keys", p)
	frame1 := renderTick(1, 3, "Gerando descrição do PR...", "Validando API keys", p)
	frame2 := renderTick(2, 3, "Gerando descrição do PR...", "Validando API keys", p)
	frame3 := renderTick(3, 3, "Gerando descrição do PR...", "Validando API keys", p)

	for _, frame := range []string{frame0, frame1, frame2, frame3} {
		require.Contains(t, frame, "\033[s")
		require.Contains(t, frame, "\033[3A")
		require.Contains(t, frame, "\033[u")
		require.Contains(t, frame, "●")
	}

	require.Contains(t, frame0, "<orange><bold>✦<reset>")
	require.Contains(t, frame0, "<orange-light>Gerando descrição do PR...<reset>")
	require.Contains(t, frame1, "<orange-dim><dim>✧<reset>")
	require.Contains(t, frame1, "<orange-dim>Gerando descrição do PR...<reset>")
	require.Contains(t, frame2, "<orange><bold>✦<reset>")
	require.Contains(t, frame3, "<orange-dim><dim>·<reset>")
}
