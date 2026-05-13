package identity

import "strings"

// SplitIdentity splits a pipe-delimited or exact-format identity string
// into a canonical name and email.
//
// Pipe-delimited format: "name1|name2|email1|email2" → first non-email part, first email part.
// Exact format: "name <email>" → name and email.
// Plain name: "name" → name and empty email.
func SplitIdentity(s string) (name, email string) {
	if s == "" {
		return "", ""
	}

	// Exact format: "name <email>".
	if idx := strings.Index(s, " <"); idx > 0 && strings.HasSuffix(s, ">") {
		return strings.TrimSpace(s[:idx]), s[idx+2 : len(s)-1]
	}

	// Pipe-delimited format.
	if strings.Contains(s, "|") {
		return splitPipeIdentity(s)
	}

	// Plain name, no email.
	return s, ""
}

func splitPipeIdentity(s string) (name, email string) {
	for part := range strings.SplitSeq(s, "|") {
		if name == "" && !strings.Contains(part, "@") {
			name = part
		}

		if email == "" && strings.Contains(part, "@") {
			email = part
		}

		if name != "" && email != "" {
			break
		}
	}

	return name, email
}
