package version

import "fmt"

var (
	Version = "dev"
	Commit  = "unknown"
	Date    = "unknown"
)

func Info() string {
	return fmt.Sprintf("%s (%s, %s)", Version, Commit, Date)
}
