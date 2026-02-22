import re

def remove_pattern(filepath, pattern):
    with open(filepath, 'r') as f:
        content = f.read()
    content = re.sub(pattern, '', content, flags=re.MULTILINE|re.DOTALL)
    with open(filepath, 'w') as f:
        f.write(content)

# Remove imports blocks from integration_test.go
integration_file = 'pkg/streaming/integration_test.go'

imports_checkpoint_pattern = r'\n\s*\{\n\s*name:\s*"imports",\n\s*setup:\s*func\(t \*testing\.T\)\s*checkpoint\.Checkpointable\s*\{\n\s*t\.Helper\(\)\n\n\s*i\s*:=\s*imports\.NewHistoryAnalyzer\(\)\n\s*require\.NoError\(t,\s*i\.Initialize\(nil\)\)\n\n\s*return\s*i\n\s*\},\n\s*\},'
remove_pattern(integration_file, imports_checkpoint_pattern)

imports_hibernate_pattern = r'\n\s*\{\n\s*name:\s*"imports",\n\s*setup:\s*func\(t \*testing\.T\)\s*streaming\.Hibernatable\s*\{\n\s*t\.Helper\(\)\n\n\s*i\s*:=\s*imports\.NewHistoryAnalyzer\(\)\n\s*require\.NoError\(t,\s*i\.Initialize\(nil\)\)\n\n\s*return\s*i\n\s*\},\n\s*\},'
remove_pattern(integration_file, imports_hibernate_pattern)

# Remove unused imports from benchmark_test.go and integration_test.go
for filepath in ['pkg/streaming/benchmark_test.go', 'pkg/streaming/integration_test.go']:
    with open(filepath, 'r') as f:
        content = f.read()
    content = re.sub(r'\n\s*"github\.com/Sumatoshi-tech/codefang/pkg/analyzers/imports"', '', content)
    with open(filepath, 'w') as f:
        f.write(content)

