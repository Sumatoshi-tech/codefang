//! Scale test: a corpus-sized (~700 rules, the largest real language) macro
//! invocation must compile without `recursion_limit` overrides — proven by the
//! expansion being pure repetition. Generated mechanically; the rule contents
//! cycle through the macro's forms so every arm is exercised at scale.

use cf_uast_mapping::{uast_language, LanguageMapping};

static BIG: LanguageMapping = uast_language! {
    name: "big",
    extensions: [".big"],
    rules: {
        rule_0 => {
            type: Synthetic,
        },
        rule_1 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_2 ("(rule_2 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_3 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_4 => {
            type: Synthetic,
        },
        rule_5 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_6 ("(rule_6 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_7 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_8 => {
            type: Synthetic,
        },
        rule_9 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_10 ("(rule_10 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_11 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_12 => {
            type: Synthetic,
        },
        rule_13 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_14 ("(rule_14 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_15 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_16 => {
            type: Synthetic,
        },
        rule_17 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_18 ("(rule_18 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_19 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_20 => {
            type: Synthetic,
        },
        rule_21 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_22 ("(rule_22 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_23 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_24 => {
            type: Synthetic,
        },
        rule_25 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_26 ("(rule_26 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_27 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_28 => {
            type: Synthetic,
        },
        rule_29 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_30 ("(rule_30 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_31 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_32 => {
            type: Synthetic,
        },
        rule_33 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_34 ("(rule_34 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_35 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_36 => {
            type: Synthetic,
        },
        rule_37 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_38 ("(rule_38 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_39 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_40 => {
            type: Synthetic,
        },
        rule_41 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_42 ("(rule_42 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_43 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_44 => {
            type: Synthetic,
        },
        rule_45 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_46 ("(rule_46 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_47 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_48 => {
            type: Synthetic,
        },
        rule_49 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_50 ("(rule_50 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_51 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_52 => {
            type: Synthetic,
        },
        rule_53 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_54 ("(rule_54 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_55 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_56 => {
            type: Synthetic,
        },
        rule_57 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_58 ("(rule_58 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_59 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_60 => {
            type: Synthetic,
        },
        rule_61 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_62 ("(rule_62 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_63 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_64 => {
            type: Synthetic,
        },
        rule_65 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_66 ("(rule_66 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_67 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_68 => {
            type: Synthetic,
        },
        rule_69 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_70 ("(rule_70 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_71 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_72 => {
            type: Synthetic,
        },
        rule_73 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_74 ("(rule_74 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_75 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_76 => {
            type: Synthetic,
        },
        rule_77 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_78 ("(rule_78 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_79 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_80 => {
            type: Synthetic,
        },
        rule_81 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_82 ("(rule_82 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_83 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_84 => {
            type: Synthetic,
        },
        rule_85 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_86 ("(rule_86 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_87 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_88 => {
            type: Synthetic,
        },
        rule_89 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_90 ("(rule_90 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_91 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_92 => {
            type: Synthetic,
        },
        rule_93 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_94 ("(rule_94 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_95 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_96 => {
            type: Synthetic,
        },
        rule_97 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_98 ("(rule_98 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_99 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_100 => {
            type: Synthetic,
        },
        rule_101 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_102 ("(rule_102 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_103 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_104 => {
            type: Synthetic,
        },
        rule_105 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_106 ("(rule_106 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_107 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_108 => {
            type: Synthetic,
        },
        rule_109 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_110 ("(rule_110 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_111 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_112 => {
            type: Synthetic,
        },
        rule_113 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_114 ("(rule_114 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_115 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_116 => {
            type: Synthetic,
        },
        rule_117 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_118 ("(rule_118 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_119 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_120 => {
            type: Synthetic,
        },
        rule_121 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_122 ("(rule_122 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_123 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_124 => {
            type: Synthetic,
        },
        rule_125 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_126 ("(rule_126 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_127 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_128 => {
            type: Synthetic,
        },
        rule_129 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_130 ("(rule_130 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_131 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_132 => {
            type: Synthetic,
        },
        rule_133 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_134 ("(rule_134 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_135 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_136 => {
            type: Synthetic,
        },
        rule_137 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_138 ("(rule_138 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_139 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_140 => {
            type: Synthetic,
        },
        rule_141 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_142 ("(rule_142 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_143 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_144 => {
            type: Synthetic,
        },
        rule_145 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_146 ("(rule_146 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_147 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_148 => {
            type: Synthetic,
        },
        rule_149 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_150 ("(rule_150 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_151 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_152 => {
            type: Synthetic,
        },
        rule_153 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_154 ("(rule_154 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_155 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_156 => {
            type: Synthetic,
        },
        rule_157 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_158 ("(rule_158 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_159 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_160 => {
            type: Synthetic,
        },
        rule_161 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_162 ("(rule_162 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_163 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_164 => {
            type: Synthetic,
        },
        rule_165 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_166 ("(rule_166 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_167 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_168 => {
            type: Synthetic,
        },
        rule_169 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_170 ("(rule_170 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_171 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_172 => {
            type: Synthetic,
        },
        rule_173 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_174 ("(rule_174 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_175 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_176 => {
            type: Synthetic,
        },
        rule_177 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_178 ("(rule_178 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_179 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_180 => {
            type: Synthetic,
        },
        rule_181 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_182 ("(rule_182 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_183 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_184 => {
            type: Synthetic,
        },
        rule_185 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_186 ("(rule_186 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_187 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_188 => {
            type: Synthetic,
        },
        rule_189 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_190 ("(rule_190 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_191 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_192 => {
            type: Synthetic,
        },
        rule_193 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_194 ("(rule_194 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_195 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_196 => {
            type: Synthetic,
        },
        rule_197 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_198 ("(rule_198 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_199 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_200 => {
            type: Synthetic,
        },
        rule_201 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_202 ("(rule_202 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_203 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_204 => {
            type: Synthetic,
        },
        rule_205 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_206 ("(rule_206 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_207 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_208 => {
            type: Synthetic,
        },
        rule_209 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_210 ("(rule_210 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_211 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_212 => {
            type: Synthetic,
        },
        rule_213 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_214 ("(rule_214 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_215 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_216 => {
            type: Synthetic,
        },
        rule_217 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_218 ("(rule_218 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_219 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_220 => {
            type: Synthetic,
        },
        rule_221 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_222 ("(rule_222 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_223 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_224 => {
            type: Synthetic,
        },
        rule_225 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_226 ("(rule_226 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_227 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_228 => {
            type: Synthetic,
        },
        rule_229 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_230 ("(rule_230 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_231 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_232 => {
            type: Synthetic,
        },
        rule_233 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_234 ("(rule_234 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_235 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_236 => {
            type: Synthetic,
        },
        rule_237 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_238 ("(rule_238 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_239 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_240 => {
            type: Synthetic,
        },
        rule_241 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_242 ("(rule_242 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_243 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_244 => {
            type: Synthetic,
        },
        rule_245 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_246 ("(rule_246 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_247 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_248 => {
            type: Synthetic,
        },
        rule_249 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_250 ("(rule_250 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_251 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_252 => {
            type: Synthetic,
        },
        rule_253 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_254 ("(rule_254 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_255 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_256 => {
            type: Synthetic,
        },
        rule_257 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_258 ("(rule_258 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_259 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_260 => {
            type: Synthetic,
        },
        rule_261 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_262 ("(rule_262 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_263 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_264 => {
            type: Synthetic,
        },
        rule_265 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_266 ("(rule_266 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_267 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_268 => {
            type: Synthetic,
        },
        rule_269 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_270 ("(rule_270 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_271 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_272 => {
            type: Synthetic,
        },
        rule_273 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_274 ("(rule_274 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_275 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_276 => {
            type: Synthetic,
        },
        rule_277 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_278 ("(rule_278 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_279 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_280 => {
            type: Synthetic,
        },
        rule_281 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_282 ("(rule_282 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_283 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_284 => {
            type: Synthetic,
        },
        rule_285 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_286 ("(rule_286 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_287 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_288 => {
            type: Synthetic,
        },
        rule_289 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_290 ("(rule_290 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_291 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_292 => {
            type: Synthetic,
        },
        rule_293 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_294 ("(rule_294 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_295 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_296 => {
            type: Synthetic,
        },
        rule_297 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_298 ("(rule_298 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_299 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_300 => {
            type: Synthetic,
        },
        rule_301 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_302 ("(rule_302 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_303 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_304 => {
            type: Synthetic,
        },
        rule_305 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_306 ("(rule_306 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_307 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_308 => {
            type: Synthetic,
        },
        rule_309 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_310 ("(rule_310 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_311 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_312 => {
            type: Synthetic,
        },
        rule_313 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_314 ("(rule_314 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_315 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_316 => {
            type: Synthetic,
        },
        rule_317 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_318 ("(rule_318 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_319 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_320 => {
            type: Synthetic,
        },
        rule_321 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_322 ("(rule_322 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_323 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_324 => {
            type: Synthetic,
        },
        rule_325 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_326 ("(rule_326 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_327 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_328 => {
            type: Synthetic,
        },
        rule_329 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_330 ("(rule_330 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_331 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_332 => {
            type: Synthetic,
        },
        rule_333 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_334 ("(rule_334 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_335 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_336 => {
            type: Synthetic,
        },
        rule_337 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_338 ("(rule_338 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_339 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_340 => {
            type: Synthetic,
        },
        rule_341 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_342 ("(rule_342 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_343 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_344 => {
            type: Synthetic,
        },
        rule_345 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_346 ("(rule_346 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_347 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_348 => {
            type: Synthetic,
        },
        rule_349 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_350 ("(rule_350 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_351 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_352 => {
            type: Synthetic,
        },
        rule_353 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_354 ("(rule_354 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_355 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_356 => {
            type: Synthetic,
        },
        rule_357 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_358 ("(rule_358 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_359 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_360 => {
            type: Synthetic,
        },
        rule_361 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_362 ("(rule_362 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_363 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_364 => {
            type: Synthetic,
        },
        rule_365 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_366 ("(rule_366 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_367 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_368 => {
            type: Synthetic,
        },
        rule_369 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_370 ("(rule_370 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_371 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_372 => {
            type: Synthetic,
        },
        rule_373 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_374 ("(rule_374 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_375 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_376 => {
            type: Synthetic,
        },
        rule_377 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_378 ("(rule_378 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_379 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_380 => {
            type: Synthetic,
        },
        rule_381 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_382 ("(rule_382 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_383 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_384 => {
            type: Synthetic,
        },
        rule_385 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_386 ("(rule_386 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_387 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_388 => {
            type: Synthetic,
        },
        rule_389 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_390 ("(rule_390 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_391 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_392 => {
            type: Synthetic,
        },
        rule_393 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_394 ("(rule_394 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_395 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_396 => {
            type: Synthetic,
        },
        rule_397 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_398 ("(rule_398 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_399 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_400 => {
            type: Synthetic,
        },
        rule_401 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_402 ("(rule_402 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_403 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_404 => {
            type: Synthetic,
        },
        rule_405 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_406 ("(rule_406 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_407 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_408 => {
            type: Synthetic,
        },
        rule_409 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_410 ("(rule_410 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_411 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_412 => {
            type: Synthetic,
        },
        rule_413 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_414 ("(rule_414 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_415 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_416 => {
            type: Synthetic,
        },
        rule_417 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_418 ("(rule_418 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_419 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_420 => {
            type: Synthetic,
        },
        rule_421 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_422 ("(rule_422 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_423 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_424 => {
            type: Synthetic,
        },
        rule_425 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_426 ("(rule_426 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_427 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_428 => {
            type: Synthetic,
        },
        rule_429 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_430 ("(rule_430 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_431 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_432 => {
            type: Synthetic,
        },
        rule_433 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_434 ("(rule_434 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_435 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_436 => {
            type: Synthetic,
        },
        rule_437 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_438 ("(rule_438 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_439 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_440 => {
            type: Synthetic,
        },
        rule_441 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_442 ("(rule_442 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_443 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_444 => {
            type: Synthetic,
        },
        rule_445 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_446 ("(rule_446 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_447 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_448 => {
            type: Synthetic,
        },
        rule_449 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_450 ("(rule_450 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_451 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_452 => {
            type: Synthetic,
        },
        rule_453 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_454 ("(rule_454 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_455 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_456 => {
            type: Synthetic,
        },
        rule_457 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_458 ("(rule_458 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_459 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_460 => {
            type: Synthetic,
        },
        rule_461 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_462 ("(rule_462 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_463 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_464 => {
            type: Synthetic,
        },
        rule_465 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_466 ("(rule_466 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_467 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_468 => {
            type: Synthetic,
        },
        rule_469 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_470 ("(rule_470 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_471 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_472 => {
            type: Synthetic,
        },
        rule_473 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_474 ("(rule_474 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_475 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_476 => {
            type: Synthetic,
        },
        rule_477 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_478 ("(rule_478 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_479 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_480 => {
            type: Synthetic,
        },
        rule_481 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_482 ("(rule_482 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_483 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_484 => {
            type: Synthetic,
        },
        rule_485 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_486 ("(rule_486 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_487 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_488 => {
            type: Synthetic,
        },
        rule_489 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_490 ("(rule_490 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_491 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_492 => {
            type: Synthetic,
        },
        rule_493 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_494 ("(rule_494 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_495 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_496 => {
            type: Synthetic,
        },
        rule_497 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_498 ("(rule_498 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_499 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_500 => {
            type: Synthetic,
        },
        rule_501 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_502 ("(rule_502 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_503 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_504 => {
            type: Synthetic,
        },
        rule_505 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_506 ("(rule_506 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_507 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_508 => {
            type: Synthetic,
        },
        rule_509 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_510 ("(rule_510 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_511 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_512 => {
            type: Synthetic,
        },
        rule_513 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_514 ("(rule_514 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_515 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_516 => {
            type: Synthetic,
        },
        rule_517 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_518 ("(rule_518 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_519 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_520 => {
            type: Synthetic,
        },
        rule_521 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_522 ("(rule_522 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_523 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_524 => {
            type: Synthetic,
        },
        rule_525 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_526 ("(rule_526 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_527 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_528 => {
            type: Synthetic,
        },
        rule_529 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_530 ("(rule_530 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_531 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_532 => {
            type: Synthetic,
        },
        rule_533 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_534 ("(rule_534 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_535 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_536 => {
            type: Synthetic,
        },
        rule_537 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_538 ("(rule_538 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_539 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_540 => {
            type: Synthetic,
        },
        rule_541 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_542 ("(rule_542 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_543 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_544 => {
            type: Synthetic,
        },
        rule_545 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_546 ("(rule_546 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_547 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_548 => {
            type: Synthetic,
        },
        rule_549 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_550 ("(rule_550 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_551 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_552 => {
            type: Synthetic,
        },
        rule_553 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_554 ("(rule_554 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_555 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_556 => {
            type: Synthetic,
        },
        rule_557 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_558 ("(rule_558 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_559 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_560 => {
            type: Synthetic,
        },
        rule_561 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_562 ("(rule_562 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_563 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_564 => {
            type: Synthetic,
        },
        rule_565 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_566 ("(rule_566 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_567 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_568 => {
            type: Synthetic,
        },
        rule_569 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_570 ("(rule_570 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_571 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_572 => {
            type: Synthetic,
        },
        rule_573 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_574 ("(rule_574 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_575 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_576 => {
            type: Synthetic,
        },
        rule_577 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_578 ("(rule_578 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_579 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_580 => {
            type: Synthetic,
        },
        rule_581 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_582 ("(rule_582 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_583 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_584 => {
            type: Synthetic,
        },
        rule_585 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_586 ("(rule_586 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_587 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_588 => {
            type: Synthetic,
        },
        rule_589 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_590 ("(rule_590 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_591 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_592 => {
            type: Synthetic,
        },
        rule_593 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_594 ("(rule_594 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_595 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_596 => {
            type: Synthetic,
        },
        rule_597 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_598 ("(rule_598 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_599 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_600 => {
            type: Synthetic,
        },
        rule_601 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_602 ("(rule_602 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_603 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_604 => {
            type: Synthetic,
        },
        rule_605 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_606 ("(rule_606 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_607 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_608 => {
            type: Synthetic,
        },
        rule_609 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_610 ("(rule_610 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_611 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_612 => {
            type: Synthetic,
        },
        rule_613 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_614 ("(rule_614 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_615 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_616 => {
            type: Synthetic,
        },
        rule_617 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_618 ("(rule_618 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_619 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_620 => {
            type: Synthetic,
        },
        rule_621 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_622 ("(rule_622 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_623 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_624 => {
            type: Synthetic,
        },
        rule_625 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_626 ("(rule_626 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_627 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_628 => {
            type: Synthetic,
        },
        rule_629 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_630 ("(rule_630 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_631 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_632 => {
            type: Synthetic,
        },
        rule_633 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_634 ("(rule_634 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_635 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_636 => {
            type: Synthetic,
        },
        rule_637 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_638 ("(rule_638 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_639 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_640 => {
            type: Synthetic,
        },
        rule_641 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_642 ("(rule_642 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_643 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_644 => {
            type: Synthetic,
        },
        rule_645 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_646 ("(rule_646 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_647 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_648 => {
            type: Synthetic,
        },
        rule_649 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_650 ("(rule_650 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_651 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_652 => {
            type: Synthetic,
        },
        rule_653 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_654 ("(rule_654 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_655 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_656 => {
            type: Synthetic,
        },
        rule_657 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_658 ("(rule_658 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_659 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_660 => {
            type: Synthetic,
        },
        rule_661 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_662 ("(rule_662 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_663 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_664 => {
            type: Synthetic,
        },
        rule_665 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_666 ("(rule_666 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_667 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_668 => {
            type: Synthetic,
        },
        rule_669 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_670 ("(rule_670 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_671 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_672 => {
            type: Synthetic,
        },
        rule_673 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_674 ("(rule_674 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_675 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_676 => {
            type: Synthetic,
        },
        rule_677 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_678 ("(rule_678 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_679 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_680 => {
            type: Synthetic,
        },
        rule_681 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_682 ("(rule_682 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_683 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_684 => {
            type: Synthetic,
        },
        rule_685 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_686 ("(rule_686 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_687 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_688 => {
            type: Synthetic,
        },
        rule_689 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_690 ("(rule_690 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_691 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_692 => {
            type: Synthetic,
        },
        rule_693 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_694 ("(rule_694 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_695 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        },
        rule_696 => {
            type: Synthetic,
        },
        rule_697 => {
            type: Call,
            token: self,
            roles: [Call],
        },
        rule_698 ("(rule_698 name: (identifier) @name)") => {
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
        },
        rule_699 => {
            type: Literal,
            token: child("identifier"),
            props: { "k": "v" },
        }
    }
};

#[test]
fn seven_hundred_rules_compile_and_convert() {
    assert_eq!(BIG.rules.len(), 700);
    let (rules, info) = BIG.to_rules();
    assert_eq!(rules.len(), 700);
    assert_eq!(info.name, "big");
    // Spot-check each cycled form.
    assert_eq!(rules[0].pattern, "(rule_0)");
    assert_eq!(rules[1].uast_spec.token, "self");
    assert_eq!(rules[2].uast_spec.token, "@name");
    assert_eq!(rules[3].uast_spec.token, "child:identifier");
    assert!(rules[3].uast_spec.props.is_some());
}
