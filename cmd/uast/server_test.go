package main

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/uast"
)

func TestHandleParseWithCustomUASTMaps(t *testing.T) {
	// Test data
	customMaps := map[string]uast.UASTMap{
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
		Language: "json", // Use json as the base language since our custom map uses json tree-sitter
		UASTMaps: customMaps,
	}

	jsonData, err := json.Marshal(request)
	if err != nil {
		t.Fatalf("Failed to marshal request: %v", err)
	}

	// Create test request
	req := httptest.NewRequest("POST", "/api/parse", bytes.NewBuffer(jsonData))
	req.Header.Set("Content-Type", "application/json")

	// Create response recorder
	w := httptest.NewRecorder()

	// Call the handler
	handleParse(w, req)

	// Check response status
	if w.Code != http.StatusOK {
		t.Errorf("Expected status 200, got %d", w.Code)
		t.Logf("Response body: %s", w.Body.String())
		return
	}

	// Parse response
	var response ParseResponse
	if err := json.Unmarshal(w.Body.Bytes(), &response); err != nil {
		t.Fatalf("Failed to unmarshal response: %v", err)
	}

	// Check for errors
	if response.Error != "" {
		t.Errorf("Expected no error, got: %s", response.Error)
		return
	}

	// Check that UAST was generated
	if response.UAST == "" {
		t.Error("Expected UAST in response, got empty string")
		return
	}

	// Verify the UAST is valid JSON
	var uastData interface{}
	if err := json.Unmarshal([]byte(response.UAST), &uastData); err != nil {
		t.Errorf("Response UAST is not valid JSON: %v", err)
	}
}

func TestHandleParseWithoutCustomUASTMaps(t *testing.T) {
	// Test without custom UAST maps (should work with built-in parsers)
	request := ParseRequest{
		Code:     `{"name": "test", "value": 42}`,
		Language: "json",
		// UASTMaps is omitted
	}

	jsonData, err := json.Marshal(request)
	if err != nil {
		t.Fatalf("Failed to marshal request: %v", err)
	}

	// Create test request
	req := httptest.NewRequest("POST", "/api/parse", bytes.NewBuffer(jsonData))
	req.Header.Set("Content-Type", "application/json")

	// Create response recorder
	w := httptest.NewRecorder()

	// Call the handler
	handleParse(w, req)

	// Check response status
	if w.Code != http.StatusOK {
		t.Errorf("Expected status 200, got %d", w.Code)
		t.Logf("Response body: %s", w.Body.String())
		return
	}

	// Parse response
	var response ParseResponse
	if err := json.Unmarshal(w.Body.Bytes(), &response); err != nil {
		t.Fatalf("Failed to unmarshal response: %v", err)
	}

	// Check for errors
	if response.Error != "" {
		t.Errorf("Expected no error, got: %s", response.Error)
		return
	}

	// Check that UAST was generated
	if response.UAST == "" {
		t.Error("Expected UAST in response, got empty string")
		return
	}
}

func TestHandleParseWithInvalidUASTMaps(t *testing.T) {
	// Test with invalid UAST maps
	customMaps := map[string]uast.UASTMap{
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

	jsonData, err := json.Marshal(request)
	if err != nil {
		t.Fatalf("Failed to marshal request: %v", err)
	}

	// Create test request
	req := httptest.NewRequest("POST", "/api/parse", bytes.NewBuffer(jsonData))
	req.Header.Set("Content-Type", "application/json")

	// Create response recorder
	w := httptest.NewRecorder()

	// Call the handler
	handleParse(w, req)

	// Check response status - should still be 200 but with error in response
	if w.Code != http.StatusOK {
		t.Errorf("Expected status 200, got %d", w.Code)
		t.Logf("Response body: %s", w.Body.String())
		return
	}

	// Parse response
	var response ParseResponse
	if err := json.Unmarshal(w.Body.Bytes(), &response); err != nil {
		t.Fatalf("Failed to unmarshal response: %v", err)
	}

	// Should have an error due to invalid UAST mapping
	if response.Error == "" {
		t.Error("Expected error for invalid UAST mapping, got none")
	}
}
