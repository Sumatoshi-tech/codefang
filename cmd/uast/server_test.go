package main

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"go.opentelemetry.io/otel/trace/noop"

	"github.com/Sumatoshi-tech/codefang/pkg/uast"
)

func TestHandleParseWithCustomUASTMaps(t *testing.T) {
	t.Parallel()

	// Test data.
	customMaps := map[string]uast.Map{
		"test_lang": {
			Extensions: []string{".test"},
			UAST: `[language "json", extensions: ".test"]

_value <- (_value) => uast(
    type: "Synthetic"
)

array <- (array) => uast(
    token: "self",
    type: "Synthetic"
)

document <- (document) => uast(
    type: "Synthetic"
)

object <- (object) => uast(
    token: "self",
    type: "Synthetic"
)

pair <- (pair) => uast(
    type: "Synthetic",
    children: "_value", "string"
)

string <- (string) => uast(
    token: "self",
    type: "Synthetic"
)
`,
		},
	}

	request := ParseRequest{
		Code:     `{"name": "test", "value": 42}`,
		Language: "json", // Use json as the base language since our custom map uses json tree-sitter.
		UASTMaps: customMaps,
	}

	jsonData, marshalErr := json.Marshal(request)
	if marshalErr != nil {
		t.Fatalf("Failed to marshal request: %v", marshalErr)
	}

	// Create test request.
	req := httptest.NewRequest(http.MethodPost, "/api/parse", bytes.NewBuffer(jsonData))
	req.Header.Set("Content-Type", "application/json")

	// Create response recorder.
	recorder := httptest.NewRecorder()

	// Call the handler.
	handleParse(recorder, req)

	// Check response status.
	if recorder.Code != http.StatusOK {
		t.Errorf("Expected status 200, got %d", recorder.Code)
		t.Logf("Response body: %s", recorder.Body.String())

		return
	}

	// Parse response.
	var response ParseResponse

	unmarshalErr := json.Unmarshal(recorder.Body.Bytes(), &response)
	if unmarshalErr != nil {
		t.Fatalf("Failed to unmarshal response: %v", unmarshalErr)
	}

	// Check for errors.
	if response.Error != "" {
		t.Errorf("Expected no error, got: %s", response.Error)

		return
	}

	// Check that UAST was generated.
	if response.UAST == "" {
		t.Error("Expected UAST in response, got empty string")

		return
	}

	// Verify the UAST is valid JSON.
	var uastData any

	uastUnmarshalErr := json.Unmarshal([]byte(response.UAST), &uastData)
	if uastUnmarshalErr != nil {
		t.Errorf("Response UAST is not valid JSON: %v", uastUnmarshalErr)
	}
}

func TestHandleParseWithoutCustomUASTMaps(t *testing.T) {
	t.Parallel()

	// Test without custom UAST maps (should work with built-in parsers).
	request := ParseRequest{
		Code:     `{"name": "test", "value": 42}`,
		Language: "json",
		// UASTMaps is omitted.
	}

	jsonData, marshalErr := json.Marshal(request)
	if marshalErr != nil {
		t.Fatalf("Failed to marshal request: %v", marshalErr)
	}

	// Create test request.
	req := httptest.NewRequest(http.MethodPost, "/api/parse", bytes.NewBuffer(jsonData))
	req.Header.Set("Content-Type", "application/json")

	// Create response recorder.
	recorder := httptest.NewRecorder()

	// Call the handler.
	handleParse(recorder, req)

	// Check response status.
	if recorder.Code != http.StatusOK {
		t.Errorf("Expected status 200, got %d", recorder.Code)
		t.Logf("Response body: %s", recorder.Body.String())

		return
	}

	// Parse response.
	var response ParseResponse

	unmarshalErr := json.Unmarshal(recorder.Body.Bytes(), &response)
	if unmarshalErr != nil {
		t.Fatalf("Failed to unmarshal response: %v", unmarshalErr)
	}

	// Check for errors.
	if response.Error != "" {
		t.Errorf("Expected no error, got: %s", response.Error)

		return
	}

	// Check that UAST was generated.
	if response.UAST == "" {
		t.Error("Expected UAST in response, got empty string")

		return
	}
}

func TestHandleParseWithInvalidUASTMaps(t *testing.T) {
	t.Parallel()

	// Test with invalid UAST maps.
	customMaps := map[string]uast.Map{
		"invalid_lang": {
			Extensions: []string{".invalid"},
			UAST:       `invalid uast mapping syntax`,
		},
	}

	request := ParseRequest{
		Code:     `{"name": "test"}`,
		Language: "invalid",
		UASTMaps: customMaps,
	}

	jsonData, marshalErr := json.Marshal(request)
	if marshalErr != nil {
		t.Fatalf("Failed to marshal request: %v", marshalErr)
	}

	// Create test request.
	req := httptest.NewRequest(http.MethodPost, "/api/parse", bytes.NewBuffer(jsonData))
	req.Header.Set("Content-Type", "application/json")

	// Create response recorder.
	recorder := httptest.NewRecorder()

	// Call the handler.
	handleParse(recorder, req)

	// Check response status -- should still be 200 but with error in response.
	if recorder.Code != http.StatusOK {
		t.Errorf("Expected status 200, got %d", recorder.Code)
		t.Logf("Response body: %s", recorder.Body.String())

		return
	}

	// Parse response.
	var response ParseResponse

	unmarshalErr := json.Unmarshal(recorder.Body.Bytes(), &response)
	if unmarshalErr != nil {
		t.Fatalf("Failed to unmarshal response: %v", unmarshalErr)
	}

	// Should have an error due to invalid UAST mapping.
	if response.Error == "" {
		t.Error("Expected error for invalid UAST mapping, got none")
	}
}

func TestUASTServer_MiddlewareWrapsRoutes(t *testing.T) {
	t.Parallel()

	tracer := noop.NewTracerProvider().Tracer("test")
	handler := newServerMux(tracer)

	req := httptest.NewRequest(http.MethodGet, "/api/mappings", http.NoBody)
	rec := httptest.NewRecorder()

	require.NotPanics(t, func() {
		handler.ServeHTTP(rec, req)
	})

	assert.Equal(t, http.StatusOK, rec.Code)
}
