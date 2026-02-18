#!/usr/bin/env python3
"""
Enhanced UAST Map Generation Script

This script generates UAST maps for all languages in the grammars directory,
ensuring all maps include proper language section headers with correct
language names and file extensions.

The script enhances generated UAST mappings with comprehensive token extraction
directives and canonical UAST types according to the UAST specification.
"""

import os
import subprocess
import json
import re
from pathlib import Path
from typing import Dict, List, Tuple, Optional


class LanguageMapper:
    """Maps language directories to their metadata"""
    
    def __init__(self):
        self._language_map = self._create_language_map()
    
    def _create_language_map(self) -> Dict[str, Tuple[str, List[str]]]:
        return {
            'go': ('go', ['.go']),
            'javascript': ('javascript', ['.js', '.jsx', '.mjs']),
            'typescript': ('typescript', ['.ts', '.tsx']),
            'tsx': ('tsx', ['.tsx']),
            'python': ('python', ['.py', '.pyw', '.pyi']),
            'java': ('java', ['.java']),
            'c': ('c', ['.c', '.h']),
            'cpp': ('cpp', ['.cpp', '.cc', '.cxx', '.hpp', '.hxx']),
            'rust': ('rust', ['.rs']),
            'rust_with_rstml': ('rust_with_rstml', ['.rs']),
            'ruby': ('ruby', ['.rb', '.rbw']),
            'php': ('php', ['.php', '.phtml']),
            'c_sharp': ('csharp', ['.cs']),
            'kotlin': ('kotlin', ['.kt', '.kts']),
            'swift': ('swift', ['.swift']),
            'scala': ('scala', ['.scala']),
            'haskell': ('haskell', ['.hs', '.lhs']),
            'ocaml': ('ocaml', ['.ml', '.mli']),
            'fsharp': ('fsharp', ['.fs', '.fsx']),
            'clojure': ('clojure', ['.clj', '.cljs']),
            'erlang': ('erlang', ['.erl']),
            'elixir': ('elixir', ['.ex', '.exs']),
            'elm': ('elm', ['.elm']),
            'bash': ('bash', ['.sh', '.bash']),
            'fish': ('fish', ['.fish']),
            'sql': ('sql', ['.sql']),
            'html': ('html', ['.html', '.htm']),
            'css': ('css', ['.css']),
            'xml': ('xml', ['.xml']),
            'json': ('json', ['.json']),
            'yaml': ('yaml', ['.yaml', '.yml']),
            'toml': ('toml', ['.toml']),
            'ini': ('ini', ['.ini']),
            'markdown': ('markdown', ['.md', '.markdown']),
            'markdown_inline': ('markdown_inline', ['.md', '.markdown']),
            'dockerfile': ('dockerfile', ['.dockerfile', 'Dockerfile']),
            'make': ('make', ['.makefile', 'Makefile']),
            'lua': ('lua', ['.lua']),
            'perl': ('perl', ['.pl', '.pm']),
            'r': ('r', ['.r', '.R']),
            'dart': ('dart', ['.dart']),
            'crystal': ('crystal', ['.cr']),
            'nim': ('nim', ['.nim']),
            'nim_format_string': ('nim_format_string', ['.nim']),
            'fortran': ('fortran', ['.f', '.f90', '.f95']),
            'commonlisp': ('commonlisp', ['.lisp', '.lsp']),
            'cmake': ('cmake', ['.cmake', 'CMakeLists.txt']),
            'tcl': ('tcl', ['.tcl']),
            'hcl': ('hcl', ['.hcl', '.tf']),
            'gotmpl': ('gotmpl', ['.gotmpl', '.go.tmpl']),
            'helm': ('helm', ['.yaml', '.yml']),
            'git_config': ('git_config', ['.gitconfig']),
            'gitignore': ('gitignore', ['.gitignore']),
            'gitattributes': ('gitattributes', ['.gitattributes']),
            'ssh_config': ('ssh_config', ['ssh_config']),
            'dotenv': ('dotenv', ['.env']),
            'properties': ('properties', ['.properties']),
            'gowork': ('gowork', ['go.work']),
            'gosum': ('gosum', ['go.sum']),
            'csv': ('csv', ['.csv']),
            'psv': ('psv', ['.psv']),
            'proxima': ('proxima', ['.proxima']),
            'proto': ('proto', ['.proto']),
            'prql': ('prql', ['.prql']),
            'latex': ('latex', ['.tex', '.ltx']),
            'ansible': ('ansible', ['.yml', '.yaml']),
        }
    
    def get_language_info(self, lang: str) -> Tuple[str, List[str], List[str]]:
        """Get language name, extensions, and files for a given language directory"""
        if lang not in self._language_map:
            return (lang, [], [])
        
        lang_name, all_patterns = self._language_map[lang]
        extensions = [p for p in all_patterns if p.startswith('.')]
        files = [p for p in all_patterns if not p.startswith('.')]
        return (lang_name, extensions, files)


class UASTTypeMapper:
    """Maps rule names to canonical UAST types according to SPEC.md"""
    
    def __init__(self):
        self._canonical_types = self._create_canonical_types()
        self._suffix_patterns = self._create_suffix_patterns()
    
    def _create_canonical_types(self) -> Dict[str, str]:
        return {
            # File and root nodes
            'file': 'File',
            'program': 'File',
            'source_file': 'File',
            
            # Identifiers
            'identifier': 'Identifier',
            'blank_identifier': 'Identifier',
            'field_identifier': 'Identifier',
            'package_identifier': 'Identifier',
            'type_identifier': 'Identifier',
            'variable_identifier': 'Identifier',

            # Literals
            'string_literal': 'Literal',
            'number_literal': 'Literal',
            'integer_literal': 'Literal',
            'float_literal': 'Literal',
            'boolean_literal': 'Literal',
            'null_literal': 'Literal',
            'imaginary_literal': 'Literal',

            # Functions and methods
            'function_declaration': 'Function',
            'function_definition': 'Function',
            'method_declaration': 'Method',
            'method_definition': 'Method',
            'function_type': 'Function',
            'lambda_expression': 'Lambda',
            'arrow_function': 'Lambda',
            'anonymous_function': 'Lambda',

            # Classes and structs
            'class_declaration': 'Class',
            'class_definition': 'Class',
            'struct_declaration': 'Struct',
            'struct_definition': 'Struct',
            'interface_declaration': 'Interface',
            'interface_definition': 'Interface',
            'enum_declaration': 'Enum',
            'enum_definition': 'Enum',
            'enum_member': 'EnumMember',

            # Variables and parameters
            'variable_declaration': 'Variable',
            'parameter': 'Parameter',
            'field_declaration': 'Field',
            'property_declaration': 'Property',
            'getter': 'Getter',
            'setter': 'Setter',

            # Control flow
            'if_statement': 'If',
            'for_statement': 'Loop',
            'while_statement': 'Loop',
            'do_while_statement': 'Loop',
            'switch_statement': 'Switch',
            'case_statement': 'Case',
            'return_statement': 'Return',
            'break_statement': 'Break',
            'continue_statement': 'Continue',
            'throw_statement': 'Throw',
            'try_statement': 'Try',
            'catch_clause': 'Catch',
            'finally_clause': 'Finally',

            # Operators
            'binary_expression': 'BinaryOp',
            'unary_expression': 'UnaryOp',
            'assignment_statement': 'Assignment',
            'assignment_expression': 'Assignment',

            # Calls and expressions
            'call_expression': 'Call',
            'function_call': 'Call',
            'method_call': 'Call',
            'await_expression': 'Await',
            'yield_expression': 'Yield',

            # Blocks and statements
            'block': 'Block',
            'expression_statement': 'Synthetic',
            'statement': 'Synthetic',

            # Imports and packages
            'import_declaration': 'Import',
            'import_statement': 'Import',
            'package_declaration': 'Package',
            'module_declaration': 'Module',
            'namespace_declaration': 'Namespace',

            # Comments and documentation
            'comment': 'Comment',
            'doc_string': 'DocString',
            'documentation_comment': 'DocString',

            # Types and annotations
            'type_annotation': 'TypeAnnotation',
            'type_declaration': 'Synthetic',
            'cast_expression': 'Cast',

            # Collections
            'array_type': 'List',
            'slice_type': 'List',
            'map_type': 'Dict',
            'list_expression': 'List',
            'dict_expression': 'Dict',
            'set_expression': 'Set',
            'tuple_expression': 'Tuple',
            'key_value_pair': 'KeyValue',
            'index_expression': 'Index',
            'slice_expression': 'Slice',
            'spread_element': 'Spread',

            # Decorators and attributes
            'decorator': 'Decorator',
            'attribute': 'Attribute',
            'annotation': 'Attribute',

            # Generators and comprehensions
            'generator_function': 'Generator',
            'list_comprehension': 'Comprehension',
            'dict_comprehension': 'Comprehension',
            'set_comprehension': 'Comprehension',

            # Pattern matching
            'pattern': 'Pattern',
            'match_statement': 'Match',

            # Keywords (Synthetic)
            'if': 'Synthetic', 'for': 'Synthetic', 'while': 'Synthetic',
            'return': 'Synthetic', 'break': 'Synthetic', 'continue': 'Synthetic',
            'goto': 'Synthetic', 'var': 'Synthetic', 'const': 'Synthetic',
            'type': 'Synthetic', 'func': 'Synthetic', 'struct': 'Synthetic',
            'interface': 'Synthetic', 'map': 'Synthetic', 'chan': 'Synthetic',
            'select': 'Synthetic', 'defer': 'Synthetic', 'go': 'Synthetic',
            'range': 'Synthetic', 'fallthrough': 'Synthetic', 'default': 'Synthetic',
            'case': 'Synthetic', 'switch': 'Synthetic', 'else': 'Synthetic',
            'true': 'Synthetic', 'false': 'Synthetic', 'nil': 'Synthetic',
        }
    
    def _create_suffix_patterns(self) -> Dict[str, str]:
        return {
            # Statements
            '_statement': 'Synthetic',
            '_expression': 'Synthetic', 
            '_declaration': 'Synthetic',
            
            # Identifiers and literals
            '_identifier': 'Identifier',
            '_literal': 'Literal',
            
            # Types and annotations
            '_type': 'Synthetic',
            '_annotation': 'TypeAnnotation',
            
            # Calls and operations
            '_call': 'Call',
            '_operation': 'Synthetic',
            '_operator': 'Synthetic',
            
            # Blocks and structures
            '_block': 'Block',
            '_body': 'Block',
            
            # Collections
            '_list': 'List',
            '_array': 'List',
            '_dict': 'Dict',
            '_map': 'Dict',
            '_set': 'Set',
            '_tuple': 'Tuple',
            
            # Functions and methods
            '_function': 'Function',
            '_method': 'Method',
            '_lambda': 'Lambda',
            
            # Classes and structs
            '_class': 'Class',
            '_struct': 'Struct',
            '_interface': 'Interface',
            '_enum': 'Enum',
            
            # Variables and fields
            '_variable': 'Variable',
            '_field': 'Field',
            '_property': 'Property',
            '_parameter': 'Parameter',
            
            # Control flow
            '_if': 'If',
            '_for': 'Loop',
            '_while': 'Loop',
            '_switch': 'Switch',
            '_case': 'Case',
            '_return': 'Return',
            '_break': 'Break',
            '_continue': 'Continue',
            '_throw': 'Throw',
            '_try': 'Try',
            '_catch': 'Catch',
            '_finally': 'Finally',
            
            # Imports and modules
            '_import': 'Import',
            '_package': 'Package',
            '_module': 'Module',
            '_namespace': 'Namespace',
            
            # Comments and documentation
            '_comment': 'Comment',
            '_doc': 'DocString',
            
            # Decorators and attributes
            '_decorator': 'Decorator',
            '_attribute': 'Attribute',
            
            # Generators and comprehensions
            '_generator': 'Generator',
            '_comprehension': 'Comprehension',
            
            # Pattern matching
            '_pattern': 'Pattern',
            '_match': 'Match',
        }
    
    def determine_uast_type(self, rule_name: str) -> str:
        """Determine the appropriate UAST type for a rule based on canonical types from SPEC.md"""
        if self._has_exact_match(rule_name):
            return self._canonical_types[rule_name]
        
        if self._has_suffix_match(rule_name):
            return self._get_suffix_type(rule_name)
        
        return "Synthetic"
    
    def _has_exact_match(self, rule_name: str) -> bool:
        return rule_name in self._canonical_types
    
    def _has_suffix_match(self, rule_name: str) -> bool:
        rule_lower = rule_name.lower()
        return any(rule_lower.endswith(suffix) for suffix in self._suffix_patterns.keys())
    
    def _get_suffix_type(self, rule_name: str) -> str:
        rule_lower = rule_name.lower()
        for suffix, uast_type in self._suffix_patterns.items():
            if rule_lower.endswith(suffix):
                return uast_type
        return "Synthetic"


class UASTRoleMapper:
    """Maps rule names and UAST types to appropriate roles according to SPEC.md"""
    
    def determine_uast_roles(self, rule_name: str, uast_type: str) -> List[str]:
        """Determine appropriate UAST roles for a rule based on canonical roles from SPEC.md"""
        roles = []
        rule_lower = rule_name.lower()
        
        if self._is_function_or_method(uast_type):
            roles.extend(['Function', 'Declaration'])
        elif self._is_class_like(uast_type):
            roles.extend(['Declaration'])
        elif self._is_variable(uast_type):
            roles.extend(['Variable', 'Declaration'])
        elif self._is_parameter(uast_type):
            roles.extend(['Parameter'])
        elif self._is_identifier(uast_type):
            roles.extend(self._get_identifier_roles(rule_lower))
        elif self._is_call(uast_type):
            roles.extend(['Call'])
        elif self._is_assignment(uast_type):
            roles.extend(['Assignment'])
        elif self._is_control_flow(uast_type):
            roles.extend(self._get_control_flow_roles(uast_type))
        elif self._is_block(uast_type):
            roles.extend(['Body'])
        elif self._is_import(uast_type):
            roles.extend(['Import'])
        elif self._is_comment_like(uast_type):
            roles.extend(self._get_comment_roles(uast_type))
        elif self._is_literal(uast_type):
            roles.extend(['Literal'])
        elif self._is_operator(uast_type):
            roles.extend(['Operator'])
        
        return roles
    
    def _is_function_or_method(self, uast_type: str) -> bool:
        return uast_type in ['Function', 'Method']
    
    def _is_class_like(self, uast_type: str) -> bool:
        return uast_type in ['Class', 'Interface', 'Struct', 'Enum']
    
    def _is_variable(self, uast_type: str) -> bool:
        return uast_type == 'Variable'
    
    def _is_parameter(self, uast_type: str) -> bool:
        return uast_type == 'Parameter'
    
    def _is_identifier(self, uast_type: str) -> bool:
        return uast_type == 'Identifier'
    
    def _is_call(self, uast_type: str) -> bool:
        return uast_type == 'Call'
    
    def _is_assignment(self, uast_type: str) -> bool:
        return uast_type == 'Assignment'
    
    def _is_control_flow(self, uast_type: str) -> bool:
        return uast_type in ['If', 'Loop', 'Switch', 'Case', 'Return', 'Break', 'Continue']
    
    def _is_block(self, uast_type: str) -> bool:
        return uast_type == 'Block'
    
    def _is_import(self, uast_type: str) -> bool:
        return uast_type == 'Import'
    
    def _is_comment_like(self, uast_type: str) -> bool:
        return uast_type in ['Comment', 'DocString']
    
    def _is_literal(self, uast_type: str) -> bool:
        return uast_type == 'Literal'
    
    def _is_operator(self, uast_type: str) -> bool:
        return uast_type in ['BinaryOp', 'UnaryOp']
    
    def _get_identifier_roles(self, rule_lower: str) -> List[str]:
        name_patterns = ['function_name', 'method_name', 'class_name']
        if any(pat in rule_lower for pat in name_patterns):
            return ['Name']
        return ['Reference']
    
    def _get_control_flow_roles(self, uast_type: str) -> List[str]:
        if uast_type == 'If':
            return ['Condition']
        elif uast_type == 'Loop':
            return ['Loop']
        elif uast_type in ['Switch', 'Case']:
            return ['Branch']
        elif uast_type in ['Return', 'Break', 'Continue']:
            return [uast_type]
        return []
    
    def _get_comment_roles(self, uast_type: str) -> List[str]:
        if uast_type == 'Comment':
            return ['Comment']
        elif uast_type == 'DocString':
            return ['Doc']
        return []


class TokenStrategyMapper:
    """Maps rule names to appropriate token extraction strategies"""
    
    def determine_token_strategy(self, rule_name: str) -> str:
        """Determine the appropriate token extraction strategy for a rule"""
        rule_lower = rule_name.lower()
        
        if self._is_identifier_like(rule_lower):
            return "self"
        elif self._is_literal_like(rule_lower):
            return "self"
        elif self._is_declaration_like(rule_lower):
            return "child:identifier"
        elif self._is_statement_like(rule_lower):
            return "self"
        elif self._is_expression_like(rule_lower):
            return "self"
        elif self._is_control_flow_keyword(rule_lower):
            return "self"
        
        return "self"
    
    def _is_identifier_like(self, rule_lower: str) -> bool:
        identifier_patterns = ['identifier', 'name', 'variable_name', 'function_name', 
                              'class_name', 'type_name', 'field_identifier', 'package_identifier']
        return any(pat in rule_lower for pat in identifier_patterns)
    
    def _is_literal_like(self, rule_lower: str) -> bool:
        literal_patterns = ['string_literal', 'number_literal', 'boolean_literal', 
                           'null_literal', 'integer_literal', 'float_literal', 'imaginary_literal']
        return any(pat in rule_lower for pat in literal_patterns)
    
    def _is_declaration_like(self, rule_lower: str) -> bool:
        declaration_patterns = ['function_declaration', 'class_declaration', 'variable_declaration', 
                               'type_declaration', 'method_declaration', 'interface_declaration', 
                               'enum_declaration', 'struct_declaration']
        return any(pat in rule_lower for pat in declaration_patterns)
    
    def _is_statement_like(self, rule_lower: str) -> bool:
        statement_patterns = ['if_statement', 'for_statement', 'while_statement', 
                             'switch_statement', 'return_statement', 'break_statement', 
                             'continue_statement', 'assignment_statement']
        return any(pat in rule_lower for pat in statement_patterns)
    
    def _is_expression_like(self, rule_lower: str) -> bool:
        expression_patterns = ['binary_expression', 'unary_expression', 'call_expression', 
                              'function_call', 'method_call']
        return any(pat in rule_lower for pat in expression_patterns)
    
    def _is_control_flow_keyword(self, rule_lower: str) -> bool:
        keywords = ['if', 'for', 'while', 'switch', 'return', 'break', 'continue', 'goto', 
                   'var', 'const', 'type', 'func', 'struct', 'interface', 'map', 'chan', 
                   'select', 'defer', 'go', 'range', 'fallthrough', 'default', 'case', 
                   'else', 'true', 'false', 'nil']
        return rule_lower in keywords


class LanguageSectionFormatter:
    """Handles formatting of language section headers in UAST maps"""
    
    def ensure_language_section(self, content: str, lang_name: str, 
                               extensions: List[str], files: List[str]) -> str:
        """Ensure the UAST map has a proper language section header"""
        if not self._has_content(content):
            return content

        content_without_header = self._remove_existing_language_section(content)
        header = self._create_language_header(lang_name, extensions, files)
        return header + content_without_header
    
    def _has_content(self, content: str) -> bool:
        return bool(content.strip())
    
    def _remove_existing_language_section(self, content: str) -> str:
        lines = content.split('\n')
        filtered_lines = []
        skip_until_empty = False

        for line in lines:
            if self._is_language_section_start(line):
                skip_until_empty = True
                continue
            if self._should_stop_skipping(skip_until_empty, line):
                skip_until_empty = False
                continue
            if skip_until_empty:
                continue
            filtered_lines.append(line)

        return '\n'.join(filtered_lines).strip()
    
    def _is_language_section_start(self, line: str) -> bool:
        return line.strip().startswith('[language ')
    
    def _should_stop_skipping(self, skip_until_empty: bool, line: str) -> bool:
        return skip_until_empty and line.strip() == ''
    
    def _create_language_header(self, lang_name: str, extensions: List[str], 
                               files: List[str]) -> str:
        header_parts = [f'[language "{lang_name}"']

        if extensions:
            extensions_str = ', '.join(f'"{ext}"' for ext in extensions)
            header_parts.append(f'extensions: {extensions_str}')

        if files:
            files_str = ', '.join(f'"{f}"' for f in files)
            header_parts.append(f'files: {files_str}')

        return ', '.join(header_parts) + ']\n\n'


class UASTMappingEnhancer:
    """Enhances UAST mappings with proper types, tokens, and roles"""
    
    def __init__(self):
        self.type_mapper = UASTTypeMapper()
        self.role_mapper = UASTRoleMapper()
        self.token_mapper = TokenStrategyMapper()
    
    def enhance_uast_mappings(self, content: str) -> str:
        """Enhance UAST mappings with correct UAST types and token extraction strategies"""
        if not self._has_content(content):
            return content

        print(f"Enhancing content with {self._count_uast_rules(content)} UAST rules...")

        lines = content.split('\n')
        enhanced_lines = []
        i = 0

        while i < len(lines):
            line = lines[i]

            if self._should_skip_line(line):
                enhanced_lines.append(line)
                i += 1
                continue

            if self._is_uast_mapping_rule(line):
                # Process the entire UAST rule block
                processed_lines, next_index = self._process_uast_rule_block(lines, i)
                enhanced_lines.extend(processed_lines)
                i = next_index
            else:
                enhanced_lines.append(line)
                i += 1

        return '\n'.join(enhanced_lines)
    
    def _process_uast_rule_block(self, lines: List[str], start_index: int) -> Tuple[List[str], int]:
        """Process a complete UAST rule block and return processed lines and next index"""
        rule_name = self._extract_rule_name(lines[start_index])
        uast_type = self.type_mapper.determine_uast_type(rule_name)
        token_strategy = self.token_mapper.determine_token_strategy(rule_name)
        uast_roles = self.role_mapper.determine_uast_roles(rule_name, uast_type)

        enhanced_lines = [lines[start_index]]
        current_index = start_index + 1

        paren_count = 0
        found_uast = False
        token_added = False
        roles_added = False
        has_token_field = False
        has_roles_field = False

        while current_index < len(lines):
            next_line = lines[current_index]

            if self._is_uast_block_start(next_line):
                found_uast = True
                paren_count = 1
            elif found_uast:
                paren_count = self._update_paren_count(paren_count, next_line)
                has_token_field = self._check_for_token_field(next_line, has_token_field)
                has_roles_field = self._check_for_roles_field(next_line, has_roles_field)
                
                next_line = self._fix_incorrect_uast_types(next_line, rule_name, uast_type)
                
                if self._should_add_token_field(has_token_field, token_added, paren_count):
                    token_line = self._create_token_line(next_line, token_strategy)
                    enhanced_lines.append(token_line)
                    token_added = True
                    print(f"  Added token strategy '{token_strategy}' for rule '{rule_name}'")
                
                if self._should_add_roles_field(has_roles_field, roles_added, uast_roles, paren_count):
                    roles_line = self._create_roles_line(next_line, uast_roles)
                    enhanced_lines.append(roles_line)
                    roles_added = True
                    print(f"  Added roles {uast_roles} for rule '{rule_name}'")

            enhanced_lines.append(next_line)
            current_index += 1

            if self._is_uast_block_end(found_uast, paren_count):
                break

        return enhanced_lines, current_index
    
    def _has_content(self, content: str) -> bool:
        return bool(content.strip())
    
    def _count_uast_rules(self, content: str) -> int:
        return len(content.split('=> uast('))
    
    def _should_skip_line(self, line: str) -> bool:
        return line.strip().startswith('[language ') or not line.strip()
    
    def _is_uast_mapping_rule(self, line: str) -> bool:
        return '=> uast(' in line
    
    def _extract_rule_name(self, line: str) -> str:
        parts = line.split('<-')
        return parts[0].strip() if len(parts) == 2 else ""
    
    def _is_uast_block_start(self, line: str) -> bool:
        return 'uast(' in line
    
    def _update_paren_count(self, paren_count: int, line: str) -> int:
        for char in line:
            if char == '(':
                paren_count += 1
            elif char == ')':
                paren_count -= 1
                if paren_count == 0:
                    break
        return paren_count
    
    def _check_for_token_field(self, line: str, has_token_field: bool) -> bool:
        return has_token_field or 'token:' in line
    
    def _check_for_roles_field(self, line: str, has_roles_field: bool) -> bool:
        return has_roles_field or 'roles:' in line
    
    def _fix_incorrect_uast_types(self, line: str, rule_name: str, uast_type: str) -> str:
        if not self._has_type_field(line):
            return line
        
        if self._has_incorrect_if_type(line, rule_name):
            return self._replace_type_in_line(line, 'If', uast_type)
        elif self._has_incorrect_synthetic_type(line, uast_type):
            return self._replace_type_in_line(line, 'Synthetic', uast_type)
        
        return line
    
    def _has_type_field(self, line: str) -> bool:
        return 'type:' in line
    
    def _has_incorrect_if_type(self, line: str, rule_name: str) -> bool:
        return 'type: "If"' in line and rule_name not in ['if', 'if_statement']
    
    def _has_incorrect_synthetic_type(self, line: str, uast_type: str) -> bool:
        return 'type: "Synthetic"' in line and uast_type != "Synthetic"
    
    def _replace_type_in_line(self, line: str, old_type: str, new_type: str) -> str:
        leading_spaces = len(line) - len(line.lstrip())
        if line.strip().endswith(','):
            return ' ' * leading_spaces + line.strip().replace(f'type: "{old_type}",', f'type: "{new_type}",')
        else:
            return ' ' * leading_spaces + line.strip().replace(f'type: "{old_type}"', f'type: "{new_type}"')
    
    def _should_add_token_field(self, has_token_field: bool, token_added: bool, paren_count: int) -> bool:
        return not has_token_field and not token_added and paren_count == 1
    
    def _should_add_roles_field(self, has_roles_field: bool, roles_added: bool, 
                               uast_roles: List[str], paren_count: int) -> bool:
        return not has_roles_field and not roles_added and uast_roles and paren_count == 1
    
    def _create_token_line(self, line: str, token_strategy: str) -> str:
        leading_spaces = len(line) - len(line.lstrip())
        return ' ' * leading_spaces + f'token: "{token_strategy}",'
    
    def _create_roles_line(self, line: str, uast_roles: List[str]) -> str:
        leading_spaces = len(line) - len(line.lstrip())
        roles_str = ', '.join(f'"{role}"' for role in uast_roles)
        return ' ' * leading_spaces + f'roles: {roles_str},'
    
    def _is_uast_block_end(self, found_uast: bool, paren_count: int) -> bool:
        return found_uast and paren_count == 0


class UASTMapValidator:
    """Validates UAST maps for proper formatting and SPEC compliance using JSON schema"""
    
    def __init__(self):
        self._schema_data = self._load_uast_schema()
        self._valid_types = self._extract_valid_types_from_schema()
        self._valid_roles = self._extract_valid_roles_from_schema()
    
    def _load_uast_schema(self) -> Dict:
        """Load the UAST JSON schema from file"""
        schema_path = 'pkg/uast/pkg/spec/uast-schema.json'
        try:
            with open(schema_path, 'r') as f:
                return json.load(f)
        except FileNotFoundError:
            raise FileNotFoundError(f"UAST schema file not found at {schema_path}. Schema is required for validation.")
        except json.JSONDecodeError as e:
            raise ValueError(f"Invalid JSON in UAST schema file: {e}")
    
    def _extract_valid_types_from_schema(self) -> set:
        """Extract valid UAST types from the JSON schema"""
        try:
            node_type_def = self._schema_data.get("definitions", {}).get("NodeType", {})
            valid_types = set(node_type_def.get("enum", []))
            if not valid_types:
                raise ValueError("No NodeType enum found in UAST schema")
            return valid_types
        except (KeyError, AttributeError) as e:
            raise ValueError(f"Could not extract NodeType enum from schema: {e}")
    
    def _extract_valid_roles_from_schema(self) -> set:
        """Extract valid UAST roles from the JSON schema"""
        try:
            role_def = self._schema_data.get("definitions", {}).get("Role", {})
            valid_roles = set(role_def.get("enum", []))
            if not valid_roles:
                raise ValueError("No Role enum found in UAST schema")
            return valid_roles
        except (KeyError, AttributeError) as e:
            raise ValueError(f"Could not extract Role enum from schema: {e}")
    
    def validate_uast_map(self, content: str, lang_name: str) -> Tuple[bool, str]:
        """Validate that the generated UAST map is properly formatted and SPEC-compliant"""
        if not self._has_content(content):
            return False, "Empty content"

        if not self._has_language_section(content):
            return False, "Missing language section"

        if not self._has_uast_mappings(content):
            return False, "No UAST mappings found"

        spec_compliance_issues = self._check_spec_compliance(content)
        if spec_compliance_issues:
            return False, f"SPEC compliance issues: {'; '.join(spec_compliance_issues)}"

        return True, "Valid and SPEC-compliant"
    
    def _has_content(self, content: str) -> bool:
        return bool(content.strip())
    
    def _has_language_section(self, content: str) -> bool:
        return content.startswith('[language ')
    
    def _has_uast_mappings(self, content: str) -> bool:
        return '=> uast(' in content
    
    def _check_spec_compliance(self, content: str) -> List[str]:
        spec_compliance_issues = []
        lines = content.split('\n')
        
        for i, line in enumerate(lines, 1):
            if self._has_type_field(line):
                uast_type = self._extract_type_value(line)
                if uast_type and not self._is_valid_type(uast_type):
                    spec_compliance_issues.append(f"Line {i}: Invalid UAST type '{uast_type}'")
            
            if self._has_roles_field(line):
                roles = self._extract_roles_value(line)
                if roles:
                    invalid_roles = [role for role in roles if not self._is_valid_role(role)]
                    if invalid_roles:
                        spec_compliance_issues.append(f"Line {i}: Invalid UAST roles {invalid_roles}")
        
        return spec_compliance_issues
    
    def _has_type_field(self, line: str) -> bool:
        return 'type:' in line
    
    def _has_roles_field(self, line: str) -> bool:
        return 'roles:' in line
    
    def _extract_type_value(self, line: str) -> Optional[str]:
        type_match = re.search(r'type:\s*"([^"]+)"', line)
        return type_match.group(1) if type_match else None
    
    def _extract_roles_value(self, line: str) -> Optional[List[str]]:
        roles_match = re.search(r'roles:\s*\[(.*?)\]', line)
        if not roles_match:
            return None
        
        roles_str = roles_match.group(1)
        # Extract quoted role names
        role_matches = re.findall(r'"([^"]+)"', roles_str)
        return role_matches if role_matches else None
    
    def _is_valid_type(self, uast_type: str) -> bool:
        return uast_type in self._valid_types
    
    def _is_valid_role(self, role: str) -> bool:
        return role in self._valid_roles


class UASTMapGenerator:
    """Main class for generating UAST maps"""
    
    def __init__(self):
        self.language_mapper = LanguageMapper()
        self.section_formatter = LanguageSectionFormatter()
        self.mapping_enhancer = UASTMappingEnhancer()
        self.validator = UASTMapValidator()
    
    def generate_uast_maps(self, grammars_dir: str, output_dir: str) -> Dict[str, List]:
        """Generate UAST maps for all languages in the grammars directory"""
        os.makedirs(output_dir, exist_ok=True)
        
        results = {
            'processed': [],
            'skipped': [],
            'errors': [],
            'validation_errors': []
        }
        
        for lang in self._get_sorted_languages(grammars_dir):
            lang_dir = os.path.join(grammars_dir, lang)
            if not self._is_language_directory(lang_dir):
                continue

            lang_name, extensions, files = self.language_mapper.get_language_info(lang)
            
            if self._should_skip_language(extensions, files):
                print(f'Skipping {lang} - no extensions or files defined')
                results['skipped'].append(lang)
                continue

            self._process_language(lang, lang_dir, lang_name, extensions, files, output_dir, results)
        
        return results
    
    def _get_sorted_languages(self, grammars_dir: str) -> List[str]:
        return sorted(os.listdir(grammars_dir))
    
    def _is_language_directory(self, lang_dir: str) -> bool:
        return os.path.isdir(lang_dir)
    
    def _should_skip_language(self, extensions: List[str], files: List[str]) -> bool:
        return not extensions and not files
    
    def _process_language(self, lang: str, lang_dir: str, lang_name: str, 
                         extensions: List[str], files: List[str], output_dir: str, 
                         results: Dict[str, List]) -> None:
        node_types_found = False
        
        for root, dirs, filenames in os.walk(lang_dir):
            for fname in filenames:
                if self._is_node_types_file(fname):
                    node_types_path = os.path.join(root, fname)
                    out_path = os.path.join(output_dir, f'{lang}.uastmap')
                    
                    self._generate_single_uast_map(lang, node_types_path, out_path, 
                                                 lang_name, extensions, files, results)
                    node_types_found = True
        
        if not node_types_found:
            results['skipped'].append(lang)
    
    def _is_node_types_file(self, filename: str) -> bool:
        return filename == 'node-types.json'
    
    def _generate_single_uast_map(self, lang: str, node_types_path: str, out_path: str,
                                 lang_name: str, extensions: List[str], files: List[str],
                                 results: Dict[str, List]) -> None:
        try:
            print(f'Generating {out_path} from {node_types_path} for language {lang_name}...')

            content = self._run_uast_command(node_types_path, lang_name, extensions)
            content = self.section_formatter.ensure_language_section(content, lang_name, extensions, files)
            
            print(f"Enhancing UAST mappings for {lang_name}...")
            content = self.mapping_enhancer.enhance_uast_mappings(content)
            print(f"Enhancement completed for {lang_name}")

            is_valid, validation_msg = self.validator.validate_uast_map(content, lang_name)
            if not is_valid:
                results['validation_errors'].append((lang, validation_msg))
                print(f"WARNING: {lang} validation failed: {validation_msg}")

            self._write_uast_map(out_path, content)
            results['processed'].append((lang, lang_name, len(extensions), len(files)))

        except subprocess.CalledProcessError as e:
            error_msg = f"Error generating {lang}: {e.stderr}"
            print(f"ERROR: {error_msg}")
            results['errors'].append((lang, error_msg))
        except Exception as e:
            error_msg = f"Unexpected error for {lang}: {str(e)}"
            print(f"ERROR: {error_msg}")
            results['errors'].append((lang, error_msg))
    
    def _run_uast_command(self, node_types_path: str, lang_name: str, extensions: List[str]) -> str:
        cmd = [
            './build/bin/uast', 'mapping', '--generate',
            '--node-types', node_types_path,
            '--language', lang_name,
            '--format', 'text'
        ]

        if extensions:
            extensions_str = ','.join(extensions)
            cmd.extend(['--extensions', extensions_str])

        result = subprocess.run(cmd, capture_output=True, text=True, check=True)
        return result.stdout
    
    def _write_uast_map(self, out_path: str, content: str) -> None:
        with open(out_path, 'w') as outf:
            outf.write(content)


class ReportGenerator:
    """Generates comprehensive reports of UAST map generation results"""
    
    def print_summary(self, results: Dict[str, List]) -> None:
        """Print comprehensive summary of UAST map generation results"""
        print(f"\n{'='*50}")
        print(f"UAST MAP GENERATION SUMMARY")
        print(f"{'='*50}")
        print(f"Successfully processed: {len(results['processed'])} languages")
        print(f"Skipped (no node-types.json): {len(results['skipped'])} languages")
        print(f"Errors: {len(results['errors'])} languages")
        print(f"Validation warnings: {len(results['validation_errors'])} languages")

        self._print_processed_languages(results['processed'])
        self._print_skipped_languages(results['skipped'])
        self._print_errors(results['errors'])
        self._print_validation_errors(results['validation_errors'])
        self._check_existing_maps()
    
    def _print_processed_languages(self, processed: List[Tuple]) -> None:
        if processed:
            print(f"\nProcessed languages:")
            for lang, lang_name, ext_count, file_count in processed:
                print(f"  - {lang} -> {lang_name} ({ext_count} extensions, {file_count} files)")
    
    def _print_skipped_languages(self, skipped: List[str]) -> None:
        if skipped:
            print(f"\nSkipped languages:")
            for lang in skipped:
                print(f"  - {lang}")
    
    def _print_errors(self, errors: List[Tuple]) -> None:
        if errors:
            print(f"\nErrors:")
            for lang, error in errors:
                print(f"  - {lang}: {error}")
    
    def _print_validation_errors(self, validation_errors: List[Tuple]) -> None:
        if validation_errors:
            print(f"\nValidation warnings:")
            for lang, msg in validation_errors:
                print(f"  - {lang}: {msg}")
    
    def _check_existing_maps(self) -> None:
        print(f"\nChecking existing UAST maps for missing language sections...")
        output_dir = 'pkg/uast/uastmaps'
        missing_sections = []
        
        for uastmap_file in os.listdir(output_dir):
            if self._is_uastmap_file(uastmap_file):
                file_path = os.path.join(output_dir, uastmap_file)
                if self._has_missing_language_section(file_path):
                    missing_sections.append(uastmap_file)

        if missing_sections:
            print(f"Files missing language sections: {len(missing_sections)}")
            for file in missing_sections:
                print(f"  - {file}")
        else:
            print("All existing UAST maps have language sections ✓")
    
    def _is_uastmap_file(self, filename: str) -> bool:
        return filename.endswith('.uastmap')
    
    def _has_missing_language_section(self, file_path: str) -> bool:
        with open(file_path, 'r') as f:
            content = f.read()
            return not content.startswith('[language ')


def main():
    """Main entry point for UAST map generation"""
    grammars_dir = 'third_party/grammars'
    output_dir = 'pkg/uast/uastmaps'
    
    generator = UASTMapGenerator()
    reporter = ReportGenerator()
    
    results = generator.generate_uast_maps(grammars_dir, output_dir)
    reporter.print_summary(results)


if __name__ == '__main__':
    main()
