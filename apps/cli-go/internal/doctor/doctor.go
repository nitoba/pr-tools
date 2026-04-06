package doctor

type Input struct {
	ConfigDirExists    bool
	ConfigDirCreatable bool
	EnvFileExists      bool
	EnvFileReadable    bool
	GoOwnedParseIssues int
	UnknownPRTKeys     []string
	Version            string
	Commit             string
	Date               string
	OS                 string
	Arch               string
}

type Report struct {
	Blocking bool
	Lines    []string
}

func Evaluate(in Input) Report {
	report := Report{}
	var lines []string

	if in.ConfigDirExists {
		lines = append(lines, "[OK] config dir exists")
	} else if in.ConfigDirCreatable {
		lines = append(lines, "[WARN] config dir missing but creatable")
	} else {
		lines = append(lines, "[ERR] config dir not creatable")
		report.Blocking = true
	}

	if in.EnvFileExists {
		if in.EnvFileReadable {
			lines = append(lines, "[OK] .env file readable")
		} else {
			lines = append(lines, "[ERR] .env file unreadable")
			report.Blocking = true
		}
	} else {
		lines = append(lines, "[WARN] .env file missing")
	}

	if in.GoOwnedParseIssues > 0 {
		lines = append(lines, "[ERR] invalid syntax in Go-owned config keys")
		report.Blocking = true
	} else if in.EnvFileExists && in.EnvFileReadable {
		lines = append(lines, "[OK] config parses successfully")
	} else if !in.EnvFileExists {
		lines = append(lines, "[OK] config not present (skipping parse)")
	}

	for _, key := range in.UnknownPRTKeys {
		lines = append(lines, "[WARN] unknown PRT key: "+key)
	}

	if in.Version != "dev" && in.Version != "unknown" {
		lines = append(lines, "[OK] version: "+in.Version)
	} else if in.Version == "dev" {
		lines = append(lines, "[WARN] running dev build (version "+in.Version+")")
	} else {
		lines = append(lines, "[WARN] version: "+in.Version)
	}

	if in.Commit != "" && in.Commit != "unknown" {
		lines = append(lines, "[OK] commit: "+in.Commit)
	}

	lines = append(lines, "[OK] runtime: "+in.OS+"/"+in.Arch)

	report.Lines = lines
	return report
}
