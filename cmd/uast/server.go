package main

import (
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"path/filepath"
	"strings"

	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
	"github.com/spf13/cobra"
)

type ParseRequest struct {
	Code     string                  `json:"code"`
	Language string                  `json:"language"`
	UASTMaps map[string]uast.UASTMap `json:"uastmaps,omitempty"`
}

type QueryRequest struct {
	UAST  string `json:"uast"`
	Query string `json:"query"`
}

type ParseResponse struct {
	UAST  string `json:"uast"`
	Error string `json:"error,omitempty"`
}

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
		Run: func(cmd *cobra.Command, args []string) {
			startServer(port, staticDir)
		},
	}

	cmd.Flags().StringVarP(&port, "port", "p", "8080", "port to listen on")
	cmd.Flags().StringVarP(&staticDir, "static", "s", "", "directory to serve static files from")

	return cmd
}

func startServer(port, staticDir string) {
	// API endpoints
	http.HandleFunc("/api/parse", handleParse)
	http.HandleFunc("/api/query", handleQuery)
	http.HandleFunc("/api/mappings", handleGetMappingsList)
	http.HandleFunc("/api/mappings/", handleGetMapping)

	// Serve static files if directory is provided
	if staticDir != "" {
		http.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
			if r.URL.Path == "/" {
				http.ServeFile(w, r, filepath.Join(staticDir, "index.html"))
			} else {
				http.ServeFile(w, r, filepath.Join(staticDir, r.URL.Path[1:]))
			}
		})
	}

	fmt.Printf("UAST Development Server starting on http://localhost:%s\n", port)
	if staticDir != "" {
		fmt.Printf("Serving static files from: %s\n", staticDir)
	}
	log.Fatal(http.ListenAndServe(":"+port, nil))
}

func handleParse(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	var req ParseRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, "Invalid request body", http.StatusBadRequest)
		return
	}

	response := ParseResponse{}

	// Initialize parser
	parser, err := uast.NewParser()
	if err != nil {
		response.Error = fmt.Sprintf("Failed to initialize parser: %v", err)
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(response)
		return
	}

	// Add custom UAST maps if provided
	if req.UASTMaps != nil && len(req.UASTMaps) > 0 {
		parser = parser.WithUASTMap(req.UASTMaps)
	}

	// Create filename with proper extension
	filename := fmt.Sprintf("input.%s", getFileExtension(req.Language))

	// Parse the code
	node, err := parser.Parse(filename, []byte(req.Code))
	if err != nil {
		response.Error = fmt.Sprintf("Parse error: %v", err)
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(response)
		return
	}

	// Assign stable IDs
	node.AssignStableIDs()

	// Convert to JSON
	nodeMap := node.ToMap()
	jsonData, err := json.MarshalIndent(nodeMap, "", "  ")
	if err != nil {
		response.Error = fmt.Sprintf("Failed to marshal UAST: %v", err)
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(response)
		return
	}

	response.UAST = string(jsonData)
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(response)
}

func handleQuery(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	var req QueryRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, "Invalid request body", http.StatusBadRequest)
		return
	}

	response := QueryResponse{}

	// Parse the UAST JSON
	var n *node.Node
	if err := json.Unmarshal([]byte(req.UAST), &n); err != nil {
		response.Error = fmt.Sprintf("Failed to parse UAST JSON: %v", err)
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(response)
		return
	}

	// Execute the query
	results, err := n.FindDSL(req.Query)
	if err != nil {
		response.Error = fmt.Sprintf("Query error: %v", err)
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(response)
		return
	}

	// Convert results to JSON
	resultsMap := nodesToMap(results)
	jsonData, err := json.MarshalIndent(resultsMap, "", "  ")
	if err != nil {
		response.Error = fmt.Sprintf("Failed to marshal results: %v", err)
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(response)
		return
	}

	response.Results = string(jsonData)
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(response)
}

func handleGetMappingsList(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	// Initialize parser to get embedded mappings list
	parser, err := uast.NewParser()
	if err != nil {
		http.Error(w, "Failed to initialize parser", http.StatusInternalServerError)
		return
	}

	mappings := parser.GetEmbeddedMappingsList()
	jsonData, err := json.MarshalIndent(mappings, "", "  ")
	if err != nil {
		http.Error(w, "Failed to marshal mappings", http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	w.Write(jsonData)
}

func handleGetMapping(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	// Extract mapping name from URL path
	parts := strings.Split(r.URL.Path, "/")
	if len(parts) < 3 { // e.g., /api/mappings/
		http.Error(w, "Invalid mapping path", http.StatusBadRequest)
		return
	}
	mappingName := parts[len(parts)-1]

	// Initialize parser to get the specific mapping
	parser, err := uast.NewParser()
	if err != nil {
		http.Error(w, "Failed to initialize parser", http.StatusInternalServerError)
		return
	}

	mapping, err := parser.GetMapping(mappingName)
	if err != nil {
		http.Error(w, fmt.Sprintf("Mapping not found: %v", err), http.StatusNotFound)
		return
	}

	jsonData, err := json.MarshalIndent(mapping, "", "  ")
	if err != nil {
		http.Error(w, "Failed to marshal mapping", http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	w.Write(jsonData)
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

	if ext, ok := extensions[strings.ToLower(language)]; ok {
		return ext
	}
	return "txt"
}
