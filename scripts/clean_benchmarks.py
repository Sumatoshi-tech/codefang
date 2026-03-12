import re
import os

benchmark_file = 'internal/streaming/benchmark_test.go'
with open(benchmark_file, 'r') as f:
    content = f.read()

patterns_to_remove = [
    r'\nfunc BenchmarkCheckpointSave_Imports.*?^}$',
    r'\nfunc BenchmarkCheckpointLoad_Imports.*?^}$',
    r'\nfunc BenchmarkHibernate_Imports.*?^}$',
    r'\nfunc BenchmarkBoot_Imports.*?^}$',
    r'\nfunc BenchmarkFork_Imports.*?^}$',
    r'\nfunc BenchmarkMerge_Imports.*?^}$'
]

for pattern in patterns_to_remove:
    content = re.sub(pattern, '', content, flags=re.MULTILINE | re.DOTALL)

with open(benchmark_file, 'w') as f:
    f.write(content)

integration_file = 'internal/streaming/integration_test.go'
with open(integration_file, 'r') as f:
    content = f.read()

# remove `{"imports", imports.NewHistoryAnalyzer()},`
content = re.sub(r'\n\s*\{"imports", imports\.NewHistoryAnalyzer\(\)\},', '', content)

# remove `case "imports": ...` block in Fork Merge test
content = re.sub(r'\n\s*case "imports":\n\s*i := imports\.NewHistoryAnalyzer\(\)\n\s*i\.Initialize\(nil\)\n\s*return i', '', content)

with open(integration_file, 'w') as f:
    f.write(content)
