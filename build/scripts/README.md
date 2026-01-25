# UAST Map Generation Scripts

This directory contains scripts for generating and maintaining UAST maps for all supported languages.

## Scripts

### `generate_all_uastmaps.py`
**Enhanced UAST Map Generation Script**

Generates UAST maps for all languages in the `grammars/` directory, ensuring all maps include proper language section headers with correct language names and file extensions.

**Features:**
- Uses Pygments for comprehensive language detection
- Ensures all generated maps have proper language sections
- Provides detailed validation and error reporting
- Comprehensive language mapping with proper extensions
- Generates summary reports

**Usage:**
```bash
python3 scripts/generate_all_uastmaps.py
```

### `fix_missing_language_sections.py`
**Language Section Fix Script**

Checks all existing UAST maps and adds language sections to any that are missing them.

**Features:**
- Scans all existing `.uastmap` files
- Automatically adds missing language sections
- Provides detailed reporting of fixes
- Safe to run multiple times

**Usage:**
```bash
python3 scripts/fix_missing_language_sections.py
```

## Language Section Format

All UAST maps should include a language section header at the beginning:

```
[language "language_name", extensions: ".ext1", ".ext2"]
```

**Examples:**
```
[language "go", extensions: ".go"]
[language "python", extensions: ".py", ".pyw", ".pyi"]
[language "javascript", extensions: ".js", ".jsx", ".mjs"]
```

## Dependencies

- Python 3.6+
- Pygments library: `pip install pygments`
- UAST binary: `./uast` (built from the project)

## Language Mapping

The scripts include comprehensive language mapping for:
- Core programming languages (Go, Python, JavaScript, etc.)
- Template languages (Go templates, Helm)
- Configuration files (Git configs, SSH config, etc.)
- Data formats (CSV, PSV, JSON, YAML, etc.)
- Documentation formats (Markdown, LaTeX)

## Error Handling

Both scripts provide comprehensive error handling:
- Missing node-types.json files
- UAST generation failures
- Validation errors
- File I/O errors

## Output

- Generated maps are saved to `pkg/uast/uastmaps/`
- Detailed console output with progress and errors
- Summary reports with statistics
- Validation results 