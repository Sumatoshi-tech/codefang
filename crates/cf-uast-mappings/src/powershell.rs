//! Mapping table for `powershell` — hand-maintained (the system of record).
//! Keep grammar-conformance, shape and Shotness language tests green when editing.

use cf_uast_mapping::{uast_language, LanguageMapping};

/// Mapping table for the `powershell` language.
pub static POWERSHELL: LanguageMapping = uast_language! {
    name: "powershell",
    extensions: [".ps1", ".psm1", ".psd1"],
    rules: {
        program => {
            type: File,
        },
        command => {
            type: Call,
            roles: [Call],
        },
        command_name => {
            type: Identifier,
            roles: [Call, Name],
        },
        command_elements => {
            type: List,
            roles: [Argument],
        },
        command_parameter => {
            type: Identifier,
            roles: [Argument],
        },
        function_statement ("(function_statement (function_name) @declared_name)") => {
            type: Function,
            roles: [Function, Declaration],
            props: { "name": "@declared_name" },
        },
        function_name => {
            type: Identifier,
            roles: [Name],
        },
        param_block => {
            type: Block,
            roles: [Parameter],
        },
        parameter_list => {
            type: Block,
            roles: [Parameter],
        },
        script_parameter => {
            type: Parameter,
            roles: [Parameter],
        },
        script_parameter_default => {
            type: Literal,
            roles: [Literal],
        },
        class_statement ("(class_statement (simple_name) @declared_name)") => {
            type: Class,
            roles: [Declaration],
            props: { "name": "@declared_name" },
        },
        class_property_definition => {
            type: Variable,
            roles: [Variable, Declaration],
        },
        class_method_definition ("(class_method_definition (simple_name) @declared_name)") => {
            type: Function,
            roles: [Function, Declaration],
            props: { "name": "@declared_name" },
        },
        class_method_parameter => {
            type: Parameter,
            roles: [Parameter],
        },
        class_method_parameter_list => {
            type: Block,
            roles: [Parameter],
        },
        script_block => {
            type: Block,
            roles: [Body],
        },
        script_block_body => {
            type: Block,
            roles: [Body],
        },
        statement_block => {
            type: Block,
            roles: [Body],
        },
        statement_list => {
            type: Block,
            roles: [Body],
        },
        if_statement => {
            type: If,
            roles: [Branch],
        },
        else_clause => {
            type: If,
            roles: [Branch],
        },
        foreach_statement => {
            type: Loop,
            roles: [Loop],
        },
        while_statement => {
            type: Loop,
            roles: [Loop],
        },
        while_condition => {
            type: Block,
            roles: [Condition],
        },
        flow_control_statement => {
            type: Break,
            roles: [Break],
        },
        pipeline => {
            type: Call,
            roles: [Call],
        },
        assignment_expression => {
            type: Assignment,
            roles: [Assignment],
        },
        left_assignment_expression => {
            type: Assignment,
            roles: [Assignment],
        },
        assignement_operator => {
            type: Identifier,
            roles: [Operator],
        },
        comparison_expression => {
            type: BinaryOp,
            roles: [Operator],
        },
        comparison_operator => {
            type: Identifier,
            roles: [Operator],
        },
        logical_expression => {
            type: BinaryOp,
            roles: [Operator],
        },
        additive_expression => {
            type: BinaryOp,
            roles: [Operator],
        },
        multiplicative_expression => {
            type: BinaryOp,
            roles: [Operator],
        },
        bitwise_expression => {
            type: BinaryOp,
            roles: [Operator],
        },
        format_expression => {
            type: BinaryOp,
            roles: [Operator],
        },
        range_expression => {
            type: BinaryOp,
            roles: [Operator],
        },
        unary_expression => {
            type: UnaryOp,
            roles: [Operator],
        },
        variable => {
            type: Identifier,
            roles: [Variable],
        },
        simple_name => {
            type: Identifier,
            roles: [Name],
        },
        generic_token => {
            type: Identifier,
            roles: [Name],
        },
        member_access => {
            type: Field,
            roles: [Member],
        },
        member_name => {
            type: Identifier,
            roles: [Name],
        },
        array_expression => {
            type: List,
            roles: [Literal],
        },
        array_literal_expression => {
            type: List,
            roles: [Literal],
        },
        hash_literal_expression => {
            type: Dict,
            roles: [Literal],
        },
        hash_literal_body => {
            type: Dict,
            roles: [Literal],
        },
        hash_entry => {
            type: KeyValue,
            roles: [Key, Value],
        },
        key_expression => {
            type: Identifier,
            roles: [Key],
        },
        integer_literal => {
            type: Literal,
            roles: [Literal],
        },
        decimal_integer_literal => {
            type: Literal,
            roles: [Literal],
        },
        string_literal => {
            type: Literal,
            roles: [Literal],
        },
        expandable_string_literal => {
            type: Literal,
            roles: [Literal],
        },
        sub_expression => {
            type: Call,
            roles: [Call],
        },
        type_literal => {
            type: TypeAnnotation,
            roles: [Type],
        },
        type_spec => {
            type: TypeAnnotation,
            roles: [Type],
        },
        type_identifier => {
            type: Identifier,
            roles: [Type],
        },
        type_name => {
            type: Identifier,
            roles: [Type],
        },
        attribute_list => {
            type: Attribute,
            roles: [Annotation],
        },
        attribute => {
            type: Attribute,
            roles: [Annotation],
        },
        comment => {
            type: Comment,
            roles: [Comment],
        },
    }
};
