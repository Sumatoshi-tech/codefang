package main

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/spf13/cobra"
	"go.opentelemetry.io/otel/trace"

	"github.com/Sumatoshi-tech/codefang/pkg/observability"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// minMappingURLParts is the minimum URL path parts for a mapping request.
const minMappingURLParts = 3

// Server timeout constants for the development HTTP server.
const (
	serverReadTimeout  = 30 * time.Second
	serverWriteTimeout = 60 * time.Second
	serverIdleTimeout  = 120 * time.Second
)

// ParseRequest holds the request body for the parse API endpoint.
type ParseRequest struct {
	UASTMaps map[string]uast.Map `json:"uastmaps,omitempty"`
	Code     string              `json:"code"`
	Language string              `json:"language"`
}

// QueryRequest holds the request body for the query API endpoint.
type QueryRequest struct {
	UAST  string `json:"uast"`
	Query string `json:"query"`
}

// ParseResponse holds the response body for the parse API endpoint.
type ParseResponse struct {
	UAST  string `json:"uast"`
	Error string `json:"error,omitempty"`
}

// QueryResponse holds the response body for the query API endpoint.
type QueryResponse struct {
	Results string `json:"results"`
	Error   string `json:"error,omitempty"`
}

func serverCmd() *cobra.Command {
	var port string

	var staticDir string

	cmd := &cobra.Command{
		Use:   "server",
		Short: "Start UAST development server",
		Long:  `Start a web server that provides UAST parsing and querying via HTTP API`,
		Run: func(_ *cobra.Command, _ []string) {
			startServer(port, staticDir)
		},
	}

	cmd.Flags().StringVarP(&port, "port", "p", "8080", "port to listen on")
	cmd.Flags().StringVarP(&staticDir, "static", "s", "", "directory to serve static files from")

	return cmd
}

// newServerMux creates the HTTP mux with all API routes wrapped in tracing middleware.
func newServerMux(tracer trace.Tracer) http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("/api/parse", handleParse)
	mux.HandleFunc("/api/query", handleQuery)
	mux.HandleFunc("/api/mappings", handleGetMappingsList)
	mux.HandleFunc("/api/mappings/", handleGetMapping)

	logger := slog.New(slog.NewTextHandler(os.Stderr, nil))

	return observability.HTTPMiddleware(tracer, logger, mux)
}

func startServer(port, staticDir string) {
	logger := slog.New(slog.NewTextHandler(os.Stderr, nil))

	cfg := observability.DefaultConfig()
	cfg.OTLPEndpoint = os.Getenv("OTEL_EXPORTER_OTLP_ENDPOINT")
	cfg.OTLPHeaders = observability.ParseOTLPHeaders(os.Getenv("OTEL_EXPORTER_OTLP_HEADERS"))

	providers, initErr := observability.Init(cfg)
	if initErr != nil {
		logger.Error("observability init failed", "error", initErr)

		return
	}

	defer func() {
		shutdownErr := providers.Shutdown(context.Background())
		if shutdownErr != nil {
			logger.Warn("observability shutdown failed", "error", shutdownErr)
		}
	}()

	handler := newServerMux(providers.Tracer)

	// Serve static files if directory is provided.
	if staticDir != "" {
		mux := http.NewServeMux()
		mux.Handle("/api/", handler)
		mux.HandleFunc("/", func(responseWriter http.ResponseWriter, request *http.Request) {
			if request.URL.Path == "/" {
				http.ServeFile(responseWriter, request, filepath.Join(staticDir, "index.html"))
			} else {
				http.ServeFile(responseWriter, request, filepath.Join(staticDir, request.URL.Path[1:]))
			}
		})

		handler = mux
	}

	logger.Info("UAST Development Server starting", "addr", "http://localhost:"+port)

	if staticDir != "" {
		logger.Info("serving static files", "dir", staticDir)
	}

	server := &http.Server{
		Addr:         ":" + port,
		Handler:      handler,
		ReadTimeout:  serverReadTimeout,
		WriteTimeout: serverWriteTimeout,
		IdleTimeout:  serverIdleTimeout,
	}

	err := server.ListenAndServe()
	if err != nil {
		logger.Error("server failed", "error", err)
	}
}

// writeJSON encodes the given value as JSON and writes it to the response writer.
func writeJSON(ctx context.Context, responseWriter http.ResponseWriter, value any) {
	responseWriter.Header().Set("Content-Type", "application/json")

	encodeErr := json.NewEncoder(responseWriter).Encode(value)
	if encodeErr != nil {
		slog.Default().ErrorContext(ctx, "failed to encode JSON response", "error", encodeErr)
	}
}

func handleParse(responseWriter http.ResponseWriter, request *http.Request) {
	if request.Method != http.MethodPost {
		http.Error(responseWriter, "Method not allowed", http.StatusMethodNotAllowed)

		return
	}

	var req ParseRequest

	decodeErr := json.NewDecoder(request.Body).Decode(&req)
	if decodeErr != nil {
		http.Error(responseWriter, "Invalid request body", http.StatusBadRequest)

		return
	}

	response := ParseResponse{}

	// Initialize parser.
	parser, err := uast.NewParser()
	if err != nil {
		response.Error = fmt.Sprintf("Failed to initialize parser: %v", err)
		writeJSON(request.Context(), responseWriter, response)

		return
	}

	// Add custom UAST maps if provided.
	if len(req.UASTMaps) > 0 {
		parser = parser.WithMap(req.UASTMaps)
	}

	// Create filename with proper extension.
	filename := fmt.Sprintf("input.%s", getFileExtension(req.Language))

	// Parse the code.
	parsedNode, parseErr := parser.Parse(request.Context(), filename, []byte(req.Code))
	if parseErr != nil {
		response.Error = fmt.Sprintf("Parse error: %v", parseErr)
		writeJSON(request.Context(), responseWriter, response)

		return
	}

	// Assign stable IDs.
	parsedNode.AssignStableIDs()

	// Convert to JSON.
	nodeMap := parsedNode.ToMap()

	jsonData, marshalErr := json.MarshalIndent(nodeMap, "", "  ")
	if marshalErr != nil {
		response.Error = fmt.Sprintf("Failed to marshal UAST: %v", marshalErr)
		writeJSON(request.Context(), responseWriter, response)

		return
	}

	response.UAST = string(jsonData)
	writeJSON(request.Context(), responseWriter, response)
}

func handleQuery(responseWriter http.ResponseWriter, request *http.Request) {
	if request.Method != http.MethodPost {
		http.Error(responseWriter, "Method not allowed", http.StatusMethodNotAllowed)

		return
	}

	var req QueryRequest

	decodeErr := json.NewDecoder(request.Body).Decode(&req)
	if decodeErr != nil {
		http.Error(responseWriter, "Invalid request body", http.StatusBadRequest)

		return
	}

	response := QueryResponse{}

	// Parse the UAST JSON.
	var parsedNode *node.Node

	unmarshalErr := json.Unmarshal([]byte(req.UAST), &parsedNode)
	if unmarshalErr != nil {
		response.Error = fmt.Sprintf("Failed to parse UAST JSON: %v", unmarshalErr)
		writeJSON(request.Context(), responseWriter, response)

		return
	}

	// Execute the query.
	results, err := parsedNode.FindDSL(req.Query)
	if err != nil {
		response.Error = fmt.Sprintf("Query error: %v", err)
		writeJSON(request.Context(), responseWriter, response)

		return
	}

	// Convert results to JSON.
	resultsMap := nodesToMap(results)

	jsonData, marshalErr := json.MarshalIndent(resultsMap, "", "  ")
	if marshalErr != nil {
		response.Error = fmt.Sprintf("Failed to marshal results: %v", marshalErr)
		writeJSON(request.Context(), responseWriter, response)

		return
	}

	response.Results = string(jsonData)
	writeJSON(request.Context(), responseWriter, response)
}

func handleGetMappingsList(responseWriter http.ResponseWriter, request *http.Request) {
	if request.Method != http.MethodGet {
		http.Error(responseWriter, "Method not allowed", http.StatusMethodNotAllowed)

		return
	}

	// Initialize parser to get embedded mappings list.
	parser, err := uast.NewParser()
	if err != nil {
		http.Error(responseWriter, "Failed to initialize parser", http.StatusInternalServerError)

		return
	}

	mappings := parser.GetEmbeddedMappingsList()
	writeJSON(request.Context(), responseWriter, mappings)
}

func handleGetMapping(responseWriter http.ResponseWriter, request *http.Request) {
	if request.Method != http.MethodGet {
		http.Error(responseWriter, "Method not allowed", http.StatusMethodNotAllowed)

		return
	}

	// Extract mapping name from URL path.
	parts := strings.Split(request.URL.Path, "/")

	if len(parts) < minMappingURLParts { // E.g., /api/mappings/.
		http.Error(responseWriter, "Invalid mapping path", http.StatusBadRequest)

		return
	}

	mappingName := parts[len(parts)-1]

	// Initialize parser to get the specific mapping.
	parser, err := uast.NewParser()
	if err != nil {
		http.Error(responseWriter, "Failed to initialize parser", http.StatusInternalServerError)

		return
	}

	mappingData, mappingErr := parser.GetMapping(mappingName)
	if mappingErr != nil {
		http.Error(responseWriter, fmt.Sprintf("Mapping not found: %v", mappingErr), http.StatusNotFound)

		return
	}

	writeJSON(request.Context(), responseWriter, mappingData)
}

func getFileExtension(language string) string {
	extensions := map[string]string{
		"go":         "go",
		"python":     "py",
		"javascript": "js",
		"typescript": "ts",
		"java":       "java",
		"cpp":        "cpp",
		"c":          "c",
		"rust":       "rs",
		"ruby":       "rb",
		"php":        "php",
		"csharp":     "cs",
		"kotlin":     "kt",
		"swift":      "swift",
		"scala":      "scala",
		"dart":       "dart",
		"lua":        "lua",
		"bash":       "sh",
		"html":       "html",
		"css":        "css",
		"json":       "json",
		"yaml":       "yaml",
		"xml":        "xml",
		"sql":        "sql",
	}

	if ext, found := extensions[strings.ToLower(language)]; found {
		return ext
	}

	return "txt"
}
