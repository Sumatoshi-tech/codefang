import React from 'react'
import { Button, ScrollArea, Badge, Flex, Text, Box } from '@radix-ui/themes'
import { Cross2Icon, CodeIcon, FileTextIcon } from '@radix-ui/react-icons'
import {
  Panel,
  PanelResizeHandle,
} from 'react-resizable-panels'

export function ExamplesSidebar({ 
  onMappingSelect, 
  onClose 
}) {
  return (
    <>
      <PanelResizeHandle style={{ 
        width: '4px', 
        backgroundColor: 'var(--gray-6)',
        cursor: 'col-resize'
      }} />
      <Panel defaultSize={20} minSize={15}>
        <Box style={{ display: 'flex', flexDirection: 'column', height: '100%', backgroundColor: 'var(--gray-1)' }}>
          <Flex 
            justify="between" 
            align="center" 
            p="2" 
            style={{ borderBottom: '1px solid var(--gray-6)' }}
          >
            <Flex align="center" gap="2">
                             <CodeIcon style={{ color: 'var(--brown-9)' }} />
              <Text size="1" weight="medium">Example Mappings</Text>
            </Flex>
            <Button
              variant="ghost"
              size="1"
              onClick={onClose}
            >
              <Cross2Icon />
            </Button>
          </Flex>
          
          <ScrollArea style={{ flex: 1 }}>
            <Box p="3">
              <Flex direction="column" gap="3">
                <Flex direction="column" gap="2">
                  <Flex align="center" gap="2">
                                         <FileTextIcon style={{ color: 'var(--brown-9)' }} />
                    <Text size="1" weight="medium">Available Examples</Text>
                    <Badge color="brown" style={{ marginLeft: 'auto' }}>3</Badge>
                  </Flex>
                  <Flex direction="column" gap="2">
                    {[
                      { 
                        title: 'Empty Custom Mapping', 
                        description: 'Creates a new empty custom mapping for you to edit',
                        type: 'Custom',
                        content: `[language "custom", extensions: ".ext"]

// Add your custom UAST mapping rules here
// Example:
// identifier <- (identifier) => uast(
//     type: "CustomIdentifier"
// )`
                      },
                      { 
                        title: 'Basic Function Mapping', 
                        description: 'Creates a custom mapping template for function detection',
                        type: 'Template',
                        content: `[language "go", extensions: ".go"]

// Basic function declaration mapping
function_declaration <- "func" identifier "(" parameter_list ")" return_type? block => uast(
    type: "FunctionDecl",
    roles: ["Function", "Declaration"],
    children: [
        identifier,
        parameter_list,
        return_type,
        block
    ]
)

identifier <- [a-zA-Z_][a-zA-Z0-9_]* => uast(
    type: "Identifier",
    roles: ["Identifier"]
)

parameter_list <- (parameter ("," parameter)*)? => uast(
    type: "ParameterList",
    roles: ["ParameterList"],
    children: [parameter]
)

return_type <- "(" type_list ")" => uast(
    type: "ReturnType",
    roles: ["ReturnType"],
    children: [type_list]
)

block <- "{" statement* "}" => uast(
    type: "Block",
    roles: ["Block"],
    children: [statement]
)`
                      },
                      { 
                        title: 'Variable Declaration Mapping', 
                        description: 'Creates a custom mapping template for variable detection',
                        type: 'Template',
                        content: `[language "go", extensions: ".go"]

// Variable declaration mapping
var_declaration <- "var" identifier type? "=" expression => uast(
    type: "VarDecl",
    roles: ["Variable", "Declaration"],
    children: [
        identifier,
        type,
        expression
    ]
)

short_var_declaration <- identifier ":=" expression => uast(
    type: "ShortVarDecl",
    roles: ["Variable", "Declaration"],
    children: [
        identifier,
        expression
    ]
)

identifier <- [a-zA-Z_][a-zA-Z0-9_]* => uast(
    type: "Identifier",
    roles: ["Identifier"]
)

type <- identifier => uast(
    type: "Type",
    roles: ["Type"]
)

expression <- identifier | literal => uast(
    type: "Expression",
    roles: ["Expression"],
    children: [identifier, literal]
)

literal <- string_literal | number_literal => uast(
    type: "Literal",
    roles: ["Literal"],
    children: [string_literal, number_literal]
)

string_literal <- "\"" [^"]* "\"" => uast(
    type: "String",
    roles: ["Literal", "String"]
)

number_literal <- [0-9]+ => uast(
    type: "Number",
    roles: ["Literal", "Number"]
)`
                      }
                    ].map((example, index) => (
                      <Button
                        key={index}
                        variant="outline"
                        size="1"
                        style={{ 
                          height: 'auto', 
                          padding: '12px',
                          flexDirection: 'column',
                          alignItems: 'flex-start',
                          gap: '8px'
                        }}
                        onClick={() => onMappingSelect(example)}
                      >
                        <Flex align="center" gap="2" style={{ width: '100%' }}>
                          <Text size="1" weight="medium">{example.title}</Text>
                          <Badge color="gray" style={{ marginLeft: 'auto' }}>{example.type}</Badge>
                        </Flex>
                        <Text 
                          size="1" 
                          color="gray" 
                          style={{ 
                            wordBreak: 'break-word',
                            lineHeight: '1.4'
                          }}
                        >
                          {example.description}
                        </Text>
                      </Button>
                    ))}
                  </Flex>
                </Flex>
              </Flex>
            </Box>
          </ScrollArea>
        </Box>
      </Panel>
    </>
  )
} 