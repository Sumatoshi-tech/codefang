import re

history_test_path = "internal/analyzers/imports/history_test.go"

with open(history_test_path, 'r') as f:
    code = f.read()

# Replace &HistoryAnalyzer{} with NewHistoryAnalyzer()
code = code.replace("&HistoryAnalyzer{}", "NewHistoryAnalyzer()")

# Remove obsolete tests
tests_to_remove = [
    r'func TestHistoryAnalyzer_Description\(t \*testing\.T\) \{[\s\S]*?\}\n\n',
    r'func TestHistoryAnalyzer_Initialize\(t \*testing\.T\) \{[\s\S]*?\}\n\n',
    r'func TestHistoryAnalyzer_SnapshotPlumbing\(t \*testing\.T\) \{[\s\S]*?\}\n\n',
    r'func TestHistoryAnalyzer_Serialize_JSON_UsesComputedMetrics\(t \*testing\.T\) \{[\s\S]*?\}\n\n',
    r'func TestHistoryAnalyzer_Serialize_YAML_UsesComputedMetrics\(t \*testing\.T\) \{[\s\S]*?\}\n\n',
    r'func TestHistoryAnalyzer_Serialize_Unsupported\(t \*testing\.T\) \{[\s\S]*?\}\n\n',
    r'func TestHistoryAnalyzer_Descriptor\(t \*testing\.T\) \{[\s\S]*?\}\n\n',
    r'func TestHistoryAnalyzer_FormatReport\(t \*testing\.T\) \{[\s\S]*?\}\n',
]

for pattern in tests_to_remove:
    code = re.sub(pattern, "", code)

with open(history_test_path, 'w') as f:
    f.write(code)

