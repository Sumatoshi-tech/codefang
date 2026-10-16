//! Mapping table for `go` — hand-maintained (the system of record).
//! Originally transpiled from the retired `go.uastmap` DSL; edit the rules
//! here and keep `tests/shape_contract.rs` green. The shape contract every
//! table honours is documented in `site/architecture/uast.md`.

use cf_uast_mapping::{uast_language, LanguageMapping};

/// Mapping table for the `go` language.
pub static GO: LanguageMapping = uast_language! {
    name: "go",
    extensions: [".go"],
    rules: {
        _expression => {
            type: Synthetic,
        },
        _simple_statement => {
            type: Synthetic,
            token: self,
        },
        _simple_type => {
            type: Synthetic,
        },
        _statement => {
            type: Synthetic,
            token: self,
        },
        _type => {
            type: Synthetic,
        },
        argument_list => {
            type: List,
            token: self,
        },
        array_type => {
            type: Synthetic,
            children: ["_expression", "_type"],
        },
        assignment_statement => {
            type: Assignment,
            token: self,
            roles: [Assignment],
            children: ["expression_list"],
        },
        binary_expression => {
            type: BinaryOp,
            roles: [Operator],
            children: ["_expression"],
        },
        block => {
            type: Block,
            token: self,
            roles: [Body],
        },
        break_statement => {
            type: Break,
            roles: [Break],
        },
        call_expression => {
            type: Call,
            token: self,
            roles: [Call],
            children: ["_expression", "argument_list", "type_arguments"],
        },
        channel_type => {
            type: Synthetic,
            children: ["_type"],
        },
        communication_case => {
            type: Case,
            token: self,
            roles: [Branch],
            children: ["receive_statement", "send_statement"],
        },
        composite_literal => {
            type: Literal,
            roles: [Literal],
            children: ["array_type", "generic_type", "implicit_length_array_type", "literal_value", "map_type", "qualified_type", "slice_type", "struct_type", "type_identifier"],
        },
        const_declaration => {
            type: Variable,
            token: self,
            roles: [Declaration, Constant],
            children: ["const_spec"],
        },
        const_spec => {
            type: Synthetic,
            children: ["_type", "expression_list", "identifier"],
        },
        continue_statement => {
            type: Continue,
            token: self,
            roles: [Continue],
        },
        dec_statement => {
            type: UnaryOp,
            roles: [Operator],
        },
        default_case => {
            type: Case,
            token: self,
            roles: [Branch],
        },
        defer_statement => {
            type: Synthetic,
        },
        dot => {
            type: Synthetic,
            token: self,
        },
        empty_statement => {
            type: Synthetic,
        },
        expression_case => {
            type: Case,
            token: self,
            roles: [Branch],
            children: ["expression_list"],
        },
        expression_list => {
            type: List,
        },
        expression_statement => {
            type: Synthetic,
            token: self,
        },
        expression_switch_statement => {
            type: Switch,
            children: ["_expression", "_simple_statement"],
        },
        fallthrough_statement => {
            type: Synthetic,
            token: self,
        },
        field_declaration => {
            type: Field,
            roles: [Member],
            children: ["_type", "field_identifier", "generic_type", "interpreted_string_literal", "qualified_type", "raw_string_literal", "type_identifier"],
        },
        field_declaration_list => {
            type: Block,
            roles: [Body],
            children: ["field_declaration"],
        },
        // The init/condition/update clause is structural: `for_statement` is
        // the one Loop.
        for_clause => {
            type: Synthetic,
            children: ["_expression", "_simple_statement"],
        },
        for_statement => {
            type: Loop,
            token: self,
            roles: [Loop],
            children: ["block"],
        },
        func_literal => {
            type: Lambda,
            roles: [Lambda],
            children: ["_simple_type", "block", "parameter_list"],
        },
        function_declaration => {
            type: Function,
            token: self,
            roles: [Function, Declaration],
            children: ["_simple_type", "block", "identifier", "parameter_list", "type_parameter_list"],
        },
        // `func(T) U` in parameter / field / variable position is a type,
        // never a Function declaration.
        function_type => {
            type: TypeAnnotation,
            roles: [Type],
            children: ["_simple_type", "parameter_list"],
        },
        generic_type => {
            type: Synthetic,
            token: self,
            children: ["negated_type", "qualified_type", "type_arguments", "type_identifier"],
        },
        go_statement => {
            type: Synthetic,
        },
        goto_statement => {
            type: Synthetic,
            token: self,
        },
        if_statement => {
            type: If,
            roles: [Condition],
            children: ["_expression", "_simple_statement", "block", "if_statement"],
        },
        implicit_length_array_type => {
            type: Synthetic,
            token: self,
            children: ["_type"],
        },
        import_declaration => {
            type: Import,
            roles: [Import],
        },
        import_spec => {
            type: Import,
            token: self,
            roles: [Import],
            children: ["blank_identifier", "dot", "interpreted_string_literal", "package_identifier", "raw_string_literal"],
        },
        import_spec_list => {
            type: Synthetic,
        },
        inc_statement => {
            type: UnaryOp,
            roles: [Operator],
        },
        index_expression => {
            type: Index,
            roles: [Index],
            children: ["_expression"],
        },
        interpreted_string_literal => {
            type: Literal,
            token: self,
            roles: [Literal],
        },
        keyed_element => {
            type: KeyValue,
            token: self,
            roles: [Key],
            children: ["literal_element"],
        },
        labeled_statement => {
            type: Synthetic,
            children: ["label_name"],
        },
        literal_element => {
            type: Literal,
            token: self,
            roles: [Literal],
        },
        // The `{...}` of a composite literal is a List, not a second Literal.
        literal_value => {
            type: List,
        },
        map_type => {
            type: Synthetic,
            token: self,
            children: ["_type"],
        },
        method_declaration => {
            type: Method,
            roles: [Function, Declaration, Member],
            children: ["_simple_type", "block", "field_identifier", "parameter_list"],
        },
        method_elem => {
            type: Method,
            token: child("identifier"),
            roles: [Function, Declaration, Member],
            children: ["_simple_type", "field_identifier", "parameter_list"],
        },
        negated_type => {
            type: Synthetic,
        },
        package_clause => {
            type: Package,
            token: self,
            roles: [Module],
        },
        parameter_declaration => {
            type: Parameter,
            roles: [Parameter],
            children: ["_type", "identifier"],
        },
        parameter_list => {
            type: Parameter,
            token: self,
            roles: [Parameter],
        },
        parenthesized_expression => {
            type: Synthetic,
        },
        parenthesized_type => {
            type: Synthetic,
            token: self,
        },
        pointer_type => {
            type: Synthetic,
        },
        qualified_type => {
            type: Synthetic,
            token: self,
            children: ["package_identifier", "type_identifier"],
        },
        range_clause => {
            type: Synthetic,
            children: ["_expression", "expression_list"],
        },
        raw_string_literal => {
            type: Literal,
            token: self,
            roles: [Literal],
        },
        receive_statement => {
            type: Synthetic,
            children: ["_expression", "expression_list"],
        },
        return_statement => {
            type: Return,
            token: self,
            roles: [Return],
        },
        select_statement => {
            type: Synthetic,
        },
        selector_expression => {
            type: Synthetic,
            token: self,
            children: ["_expression", "field_identifier"],
        },
        send_statement => {
            type: Synthetic,
            children: ["_expression"],
        },
        short_var_declaration => {
            type: Variable,
            token: self,
            roles: [Variable, Declaration],
            children: ["expression_list"],
        },
        slice_expression => {
            type: Slice,
            children: ["_expression"],
        },
        slice_type => {
            type: Slice,
            token: self,
            children: ["_type"],
        },
        source_file => {
            type: File,
        },
        type_alias => {
            type: Synthetic,
            children: ["_type", "type_identifier"],
        },
        type_arguments => {
            type: Synthetic,
            token: self,
        },
        type_assertion_expression => {
            type: Synthetic,
            children: ["_expression", "_type"],
        },
        type_case => {
            type: Case,
            token: self,
            roles: [Branch],
            children: ["_type"],
        },
        type_constraint => {
            type: Synthetic,
        },
        type_conversion_expression => {
            type: Synthetic,
            token: self,
            children: ["_expression", "_type"],
        },
        type_declaration => {
            type: Synthetic,
        },
        type_elem => {
            type: Synthetic,
            token: child("identifier"),
        },
        type_instantiation_expression => {
            type: Synthetic,
            children: ["_type"],
        },
        type_parameter_declaration => {
            type: Parameter,
            token: self,
            roles: [Parameter],
            children: ["identifier", "type_constraint"],
        },
        type_parameter_list => {
            type: Parameter,
            roles: [Parameter],
        },
        // `type X struct{...}` / `type X interface{...}` are the Struct /
        // Interface declarations (named through the `name` field); any other
        // `type X T` is a TypeDeclaration. The inner `struct_type` /
        // `interface_type` nodes are unmapped, so a declaration is never
        // counted twice and an anonymous `struct{}` is not a declaration.
        type_spec_struct ("(type_spec)") => {
            type: Struct,
            roles: [Struct, Declaration],
            children: ["_type", "type_identifier", "type_parameter_list"],
            when: ["type.type == \"struct_type\""],
        },
        type_spec_interface ("(type_spec)") => {
            type: Interface,
            roles: [Interface, Declaration],
            children: ["_type", "type_identifier", "type_parameter_list"],
            when: ["type.type == \"interface_type\""],
        },
        type_spec => {
            type: TypeDeclaration,
            roles: [Declaration, Type],
            children: ["_type", "type_identifier", "type_parameter_list"],
        },
        type_switch_statement => {
            type: Switch,
            children: ["_expression", "_simple_statement", "expression_list"],
        },
        unary_expression => {
            type: UnaryOp,
            token: self,
            roles: [Operator],
            children: ["_expression"],
        },
        var_declaration => {
            type: Variable,
            roles: [Variable, Declaration],
        },
        var_spec => {
            type: Variable,
            token: self,
            roles: [Variable, Declaration],
            children: ["_type", "expression_list", "identifier"],
        },
        var_spec_list => {
            type: List,
        },
        variadic_argument => {
            type: List,
            token: self,
        },
        variadic_parameter_declaration => {
            type: Parameter,
            roles: [Parameter],
            children: ["_type", "identifier"],
        },
        blank_identifier => {
            type: Synthetic,
            token: self,
        },
        break => {
            type: Break,
            roles: [Break],
        },
        case => {
            type: Case,
            token: self,
            roles: [Branch],
        },
        chan => {
            type: Synthetic,
        },
        comment => {
            type: Comment,
            token: self,
            roles: [Comment],
        },
        const => {
            type: Synthetic,
        },
        continue => {
            type: Continue,
            token: self,
            roles: [Continue],
        },
        default => {
            type: Synthetic,
        },
        defer => {
            type: Synthetic,
            token: self,
        },
        else => {
            type: Synthetic,
        },
        escape_sequence => {
            type: Synthetic,
            token: self,
        },
        fallthrough => {
            type: Synthetic,
        },
        false => {
            type: Literal,
            token: self,
            roles: [Literal],
        },
        field_identifier => {
            type: Identifier,
            token: self,
            roles: [Name],
        },
        float_literal => {
            type: Literal,
            token: self,
            roles: [Literal],
        },
        for => {
            type: Loop,
            roles: [Loop],
        },
        func => {
            type: Synthetic,
            token: self,
        },
        go => {
            type: Synthetic,
        },
        goto => {
            type: Synthetic,
            token: self,
        },
        identifier => {
            type: Identifier,
            token: self,
            roles: [Name],
        },
        if => {
            type: If,
            token: self,
        },
        imaginary_literal => {
            type: Literal,
            token: self,
            roles: [Literal],
        },
        import => {
            type: Synthetic,
            token: self,
        },
        int_literal => {
            type: Literal,
            token: self,
            roles: [Literal],
        },
        interface => {
            type: Interface,
            token: self,
            roles: [Interface, Declaration],
        },
        iota => {
            type: Synthetic,
            token: self,
        },
        label_name => {
            type: Synthetic,
        },
        map => {
            type: Synthetic,
            token: self,
        },
        nil => {
            type: Literal,
            token: self,
            roles: [Literal],
        },
        package => {
            type: Package,
            token: self,
            roles: [Module],
        },
        package_identifier => {
            type: Identifier,
            token: self,
            roles: [Name],
        },
        range => {
            type: Identifier,
            token: self,
            roles: [Reference],
        },
        return => {
            type: Return,
            token: self,
            roles: [Return],
        },
        rune_literal => {
            type: Literal,
            token: self,
            roles: [Literal],
        },
        select => {
            type: Synthetic,
            token: self,
        },
        struct => {
            type: Struct,
            roles: [Struct, Declaration],
        },
        switch => {
            type: Switch,
            token: self,
        },
        true => {
            type: Literal,
            token: self,
            roles: [Literal],
        },
        type => {
            type: Synthetic,
            token: self,
        },
        type_identifier => {
            type: Identifier,
            token: self,
            roles: [Name, Type],
        },
        var => {
            type: Identifier,
            token: self,
            roles: [Reference],
        },
    }
};
