package observability

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
)

const (
	healthStatusOK          = "ok"
	healthStatusUnavailable = "unavailable"
)

// ReadyCheck is a function that checks if a subsystem is ready.
// It returns nil if the check passes, or an error describing the failure.
type ReadyCheck func(ctx context.Context) error

// HealthHandler returns an [http.Handler] for liveness checks at /healthz.
// It always returns HTTP 200 with {"status":"ok"}.
func HealthHandler() http.Handler {
	return http.HandlerFunc(func(rw http.ResponseWriter, _ *http.Request) {
		rw.Header().Set("Content-Type", "application/json")
		rw.WriteHeader(http.StatusOK)
		writeHealthJSON(rw, healthStatusOK)
	})
}

// ReadyHandler returns an [http.Handler] for readiness checks at /readyz.
// It runs all provided checks; if any fail, it returns HTTP 503 with {"status":"unavailable"}.
// If no checks are provided or all pass, it returns HTTP 200 with {"status":"ok"}.
func ReadyHandler(checks ...ReadyCheck) http.Handler {
	return http.HandlerFunc(func(rw http.ResponseWriter, hr *http.Request) {
		rw.Header().Set("Content-Type", "application/json")

		for _, check := range checks {
			err := check(hr.Context())
			if err != nil {
				rw.WriteHeader(http.StatusServiceUnavailable)
				writeHealthJSON(rw, healthStatusUnavailable)

				return
			}
		}

		rw.WriteHeader(http.StatusOK)
		writeHealthJSON(rw, healthStatusOK)
	})
}

func writeHealthJSON(w io.Writer, status string) {
	data, err := json.Marshal(map[string]string{"status": status})
	if err != nil {
		return
	}

	writeOrDiscard(w, data)
}

func writeOrDiscard(w io.Writer, data []byte) {
	_, err := w.Write(data)
	if err != nil {
		return
	}
}
