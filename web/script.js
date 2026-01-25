// Global variables
let codeEditor;
let currentUAST = '';

// Initialize the application
document.addEventListener('DOMContentLoaded', function() {
    initializeCodeEditor();
    setupEventListeners();
    loadExampleCode();
});

function initializeCodeEditor() {
    // Initialize CodeMirror for the code input
    codeEditor = CodeMirror.fromTextArea(document.getElementById('codeInput'), {
        mode: 'javascript',
        theme: 'monokai',
        lineNumbers: true,
        autoCloseBrackets: true,
        matchBrackets: true,
        indentUnit: 4,
        tabSize: 4,
        lineWrapping: true,
        foldGutter: true,
        gutters: ['CodeMirror-linenumbers', 'CodeMirror-foldgutter']
    });

    // Set initial size
    codeEditor.setSize('100%', '300px');
}

function setupEventListeners() {
    // Parse button
    document.getElementById('parseBtn').addEventListener('click', parseCode);
    
    // Query button
    document.getElementById('queryBtn').addEventListener('click', executeQuery);
    
    // Language change
    document.getElementById('language').addEventListener('change', onLanguageChange);
    
    // Example query clicks
    document.querySelectorAll('.example code').forEach(code => {
        code.addEventListener('click', function() {
            document.getElementById('queryInput').value = this.textContent;
        });
    });

    // Enter key in query input
    document.getElementById('queryInput').addEventListener('keypress', function(e) {
        if (e.key === 'Enter') {
            executeQuery();
        }
    });
}

function onLanguageChange() {
    const language = document.getElementById('language').value;
    const modeMap = {
        'go': 'text/x-go',
        'python': 'text/x-python',
        'javascript': 'javascript',
        'typescript': 'text/typescript',
        'java': 'text/x-java-source',
        'cpp': 'text/x-c++src',
        'c': 'text/x-csrc',
        'rust': 'text/x-rustsrc',
        'ruby': 'text/x-ruby',
        'php': 'text/x-php',
        'csharp': 'text/x-csharp',
        'kotlin': 'text/x-kotlin',
        'swift': 'text/x-swift',
        'scala': 'text/x-scala',
        'dart': 'text/x-dart',
        'lua': 'text/x-lua',
        'bash': 'text/x-sh',
        'html': 'text/html',
        'css': 'css',
        'json': 'application/json',
        'yaml': 'text/x-yaml',
        'xml': 'application/xml',
        'sql': 'text/x-sql'
    };

    const mode = modeMap[language] || 'text/plain';
    codeEditor.setOption('mode', mode);
}

function loadExampleCode() {
    const language = document.getElementById('language').value;
    const examples = {
        'go': `package main

import (
    "fmt"
    "os"
)

func main() {
    fmt.Println("Hello, World!")
}`,
        'python': `import os
import sys
from typing import List

def main():
    print("Hello, World!")
    
if __name__ == "__main__":
    main()`,
        'javascript': `import { readFile } from 'fs';
import path from 'path';

function main() {
    console.log("Hello, World!");
}

export default main;`,
        'java': `import java.util.List;
import java.util.ArrayList;

public class Main {
    public static void main(String[] args) {
        System.out.println("Hello, World!");
    }
}`,
        'cpp': `#include <iostream>
#include <vector>

int main() {
    std::cout << "Hello, World!" << std::endl;
    return 0;
}`,
        'rust': `use std::io;

fn main() {
    println!("Hello, World!");
}`,
        'ruby': `require 'json'
require 'net/http'

def main
  puts "Hello, World!"
end

main`,
        'php': `<?php

use Symfony\\Component\\HttpFoundation\\Request;

function main() {
    echo "Hello, World!";
}

main();`,
        'csharp': `using System;
using System.Collections.Generic;

namespace MyApp
{
    class Program
    {
        static void Main(string[] args)
        {
            Console.WriteLine("Hello, World!");
        }
    }
}`,
        'kotlin': `import kotlin.io.println

fun main() {
    println("Hello, World!")
}`,
        'swift': `import Foundation

func main() {
    print("Hello, World!")
}

main()`,
        'scala': `import scala.io.StdIn

object Main {
  def main(args: Array[String]): Unit = {
    println("Hello, World!")
  }
}`,
        'dart': `import 'dart:io';

void main() {
  print('Hello, World!');
}`,
        'lua': `local io = require("io")

function main()
    print("Hello, World!")
end

main()`,
        'bash': `#!/bin/bash

echo "Hello, World!"`,
        'html': `<!DOCTYPE html>
<html>
<head>
    <title>Hello World</title>
</head>
<body>
    <h1>Hello, World!</h1>
</body>
</html>`,
        'css': `body {
    font-family: Arial, sans-serif;
    margin: 0;
    padding: 20px;
}

h1 {
    color: #333;
}`,
        'json': `{
  "name": "example",
  "version": "1.0.0",
  "dependencies": {
    "express": "^4.17.1"
  }
}`,
        'yaml': `name: example
version: 1.0.0
dependencies:
  express: ^4.17.1`,
        'xml': `<?xml version="1.0" encoding="UTF-8"?>
<root>
    <item>Hello, World!</item>
</root>`,
        'sql': `SELECT * FROM users 
WHERE age > 18 
ORDER BY name;`
    };

    const example = examples[language] || '// Enter your code here...';
    codeEditor.setValue(example);
}

async function parseCode() {
    const parseBtn = document.getElementById('parseBtn');
    const uastOutput = document.getElementById('uastOutput');
    
    // Show loading state
    parseBtn.textContent = 'Parsing...';
    parseBtn.disabled = true;
    uastOutput.textContent = 'Parsing code...';
    
    try {
        const code = codeEditor.getValue();
        const language = document.getElementById('language').value;
        
        if (!code.trim()) {
            throw new Error('Please enter some code to parse');
        }
        
        const response = await fetch('/api/parse', {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({
                code: code,
                language: language
            })
        });
        
        const data = await response.json();
        
        if (data.error) {
            throw new Error(data.error);
        }
        
        currentUAST = data.uast;
        uastOutput.textContent = formatJSON(data.uast);
        
        // Show success message
        showMessage('Code parsed successfully!', 'success');
        
    } catch (error) {
        uastOutput.textContent = `Error: ${error.message}`;
        showMessage(`Parse error: ${error.message}`, 'error');
    } finally {
        // Reset button state
        parseBtn.textContent = 'Parse Code';
        parseBtn.disabled = false;
    }
}

async function executeQuery() {
    const queryBtn = document.getElementById('queryBtn');
    const queryOutput = document.getElementById('queryOutput');
    const queryInput = document.getElementById('queryInput');
    
    if (!currentUAST) {
        showMessage('Please parse some code first', 'error');
        return;
    }
    
    const query = queryInput.value.trim();
    if (!query) {
        showMessage('Please enter a query', 'error');
        return;
    }
    
    // Show loading state
    queryBtn.textContent = 'Executing...';
    queryBtn.disabled = true;
    queryOutput.textContent = 'Executing query...';
    
    try {
        const response = await fetch('/api/query', {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({
                uast: currentUAST,
                query: query
            })
        });
        
        const data = await response.json();
        
        if (data.error) {
            throw new Error(data.error);
        }
        
        queryOutput.textContent = formatJSON(data.results);
        showMessage('Query executed successfully!', 'success');
        
    } catch (error) {
        queryOutput.textContent = `Error: ${error.message}`;
        showMessage(`Query error: ${error.message}`, 'error');
    } finally {
        // Reset button state
        queryBtn.textContent = 'Execute Query';
        queryBtn.disabled = false;
    }
}

function formatJSON(jsonString) {
    try {
        const parsed = JSON.parse(jsonString);
        return JSON.stringify(parsed, null, 2);
    } catch (e) {
        return jsonString;
    }
}

function showMessage(message, type) {
    // Remove existing messages
    const existingMessages = document.querySelectorAll('.message');
    existingMessages.forEach(msg => msg.remove());
    
    // Create new message
    const messageDiv = document.createElement('div');
    messageDiv.className = `message ${type}`;
    messageDiv.textContent = message;
    
    // Add to page
    document.body.appendChild(messageDiv);
    
    // Auto-remove after 5 seconds
    setTimeout(() => {
        if (messageDiv.parentNode) {
            messageDiv.remove();
        }
    }, 5000);
}

// Add message styles
const style = document.createElement('style');
style.textContent = `
    .message {
        position: fixed;
        top: 20px;
        right: 20px;
        padding: 15px 20px;
        border-radius: 5px;
        color: white;
        font-weight: 600;
        z-index: 1000;
        animation: slideIn 0.3s ease-out;
    }
    
    .message.success {
        background: #27ae60;
    }
    
    .message.error {
        background: #e74c3c;
    }
    
    @keyframes slideIn {
        from {
            transform: translateX(100%);
            opacity: 0;
        }
        to {
            transform: translateX(0);
            opacity: 1;
        }
    }
`;
document.head.appendChild(style); 