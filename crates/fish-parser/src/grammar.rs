use crate::ast::*;

#[derive(Debug)]
enum CommandItem {
    Arg(Word),
    Redir(Redirection),
}

#[derive(Debug)]
enum FuncOpt {
    Args(Vec<String>),
    Desc(String),
}

peg::parser! {
    pub grammar fish_grammar() for str {
        pub rule program() -> Program
            = statement_sep()? shebang:shebang_line()? statements:statement_list() _* ![_] {
                Program { shebang, statements }
            }

        rule shebang_line() -> String
            = "#!" s:$( [^'\n']* ) ("\n" / ![_]) {
                format!("#!{}", s)
            }

        pub rule statement_list() -> Vec<Statement>
            = statement_sep()? stmts:(s:statement() ** statement_sep()) statement_sep()? {
                stmts.into_iter().filter(|s| match s {
                    Statement::Comment(_) => true,
                    Statement::Pipeline(p) => !p.commands.is_empty(),
                    _ => true,
                }).collect()
            }

        rule statement_sep()
            = (_* (['\n' | ';'] / ("\\" "\n")))+ _*

        rule _() = [' ' | '\t']

        rule ident() -> String
            = s:$(['a'..='z' | 'A'..='Z' | '_']['a'..='z' | 'A'..='Z' | '0'..='9' | '_']*) { s.to_string() }

        rule keyword_char() = ['a'..='z' | 'A'..='Z' | '0'..='9' | '_']

        rule reserved_keyword()
            = ("if" / "else" / "switch" / "case" / "for" / "while" / "function" / "begin" / "end") !keyword_char()

        rule block_terminator()
            = ("end" / "else" / "case") !keyword_char()
        pub rule statement() -> Statement
            = !block_terminator() s:inner_statement() { s }

        rule inner_statement() -> Statement
            = comment()
            / return_stmt()
            / break_stmt()
            / continue_stmt()
            / if_stmt()
            / switch_stmt()
            / for_stmt()
            / while_stmt()
            / function_stmt()
            / begin_stmt()
            / pipeline_stmt()

        rule comment() -> Statement
            = "#" s:$((!['\n'][_])*) {
                Statement::Comment(s.to_string())
            }
        rule return_stmt() -> Statement
            = "return" !keyword_char() _+ w:word() { Statement::Return(Some(w)) }
            / "return" !keyword_char() { Statement::Return(None) }

        rule break_stmt() -> Statement
            = "break" !keyword_char() { Statement::Break }

        rule continue_stmt() -> Statement
            = "continue" !keyword_char() { Statement::Continue }
        rule pipeline_stmt() -> Statement
            = p:pipeline() { Statement::Pipeline(p) }

        rule pipeline() -> Pipeline
            = !reserved_keyword() comb:combinator_prefix()? _* negate:("not" !keyword_char() _+)? cmds:(command() ++ (_* "|" _*)) bg:(_* "&")? {
                let mut commands = cmds;
                if let Some(true) = negate.map(|_| true) {
                    if let Some(first) = commands.first_mut() {
                        first.negate = true;
                    }
                }
                Pipeline {
                    commands,
                    combinator: comb.unwrap_or(Combinator::None),
                    background: bg.is_some(),
                }
            }

        rule combinator_prefix() -> Combinator
            = "and" !keyword_char() _+ { Combinator::And }
            / "or" !keyword_char() _+ { Combinator::Or }
            / "&&" _* { Combinator::And }
            / "||" _* { Combinator::Or }

        rule command() -> Command
            = items:(command_item() ++ (_+)) {
                let mut args = Vec::new();
                let mut redirections = Vec::new();
                for item in items {
                    match item {
                        CommandItem::Arg(w) => args.push(w),
                        CommandItem::Redir(r) => redirections.push(r),
                    }
                }
                Command {
                    negate: false,
                    args,
                    redirections,
                }
            }

        rule command_item() -> CommandItem
            = r:redirection() { CommandItem::Redir(r) }
            / w:word() { CommandItem::Arg(w) }

        rule redirection() -> Redirection
            = fd:(n:$(['0'..='9']+) { n.parse::<u32>().unwrap() })? mode:redirect_mode() _* target:word() {
                Redirection { fd, mode, target }
            }

        rule redirect_mode() -> RedirectMode
            = ">>" { RedirectMode::Append }
            / ">&" { RedirectMode::DupOutput }
            / "<&" { RedirectMode::DupInput }
            / ">" { RedirectMode::Output }
            / "<" { RedirectMode::Input }
            / "^^" { RedirectMode::AppendAndErr }
            / "^" { RedirectMode::OutputAndErr }
            / "&>>" { RedirectMode::AppendAndErr }
            / "&>" { RedirectMode::OutputAndErr }

        rule word() -> Word
            = parts:(word_part()+ ) {
                Word { parts }
            }

        rule word_part() -> WordPart
            = single_quoted()
            / double_quoted()
            / variable_ref()
            / command_subst()
            / brace_expansion()
            / literal()

        rule literal() -> WordPart
            = s:$( ( [^' ' | '\t' | '\n' | ';' | '|' | '&' | '<' | '>' | '^' | '(' | ')' | '{' | '}' | '$' | '\'' | '\"' | '#'] / ("\\" [_]) )+ ) {
                WordPart::Literal(unescape(s))
            }

        rule single_quoted() -> WordPart
            = "'" s:$(( [^'\''] / "\\'" )*) "'" {
                WordPart::SingleQuoted(s.replace("\\'", "'"))
            }

        rule double_quoted() -> WordPart
            = "\"" parts:double_quoted_part()* "\"" {
                WordPart::DoubleQuoted(parts)
            }

        rule double_quoted_part() -> WordPart
            = variable_ref()
            / command_subst()
            / s:$(( [^'\"' | '$' | '(' | '\\'] / ("\\" [_]) )+) {
                WordPart::Literal(unescape(s))
            }

        rule variable_ref() -> WordPart
            = "$" name:$(['a'..='z' | 'A'..='Z' | '_']['a'..='z' | 'A'..='Z' | '0'..='9' | '_']*) slices:slice()* {
                WordPart::Variable(VariableRef {
                    name: name.to_string(),
                    slices,
                })
            }

        rule slice() -> Slice
            = "[" _* start:slice_index()? _* ".." _* end:slice_index()? _* "]" {
                Slice::Range { start, end }
            }
            / "[" _* idx:slice_index() _* "]" {
                Slice::Index(idx)
            }

        rule slice_index() -> SliceIndex
            = "-" n:$(['0'..='9']+) { SliceIndex::Neg(n.parse::<usize>().unwrap()) }
            / n:$(['0'..='9']+) { SliceIndex::Pos(n.parse::<usize>().unwrap()) }

        rule command_subst() -> WordPart
            = "$(" _* stmts:statement_list() _* ")" {
                WordPart::CommandSubst(stmts)
            }
            / "(" _* stmts:statement_list() _* ")" {
                WordPart::CommandSubst(stmts)
            }

        rule brace_expansion() -> WordPart
            = "{" _* words:(word() ** (_* "," _*)) _* "}" {
                WordPart::BraceExpansion(words)
            }

        rule if_stmt() -> Statement
            = "if" !keyword_char() _+ cond:pipeline() statement_sep()
              then_body:statement_list()
              elifs:elif_branch()*
              else_body:else_branch()?
              "end" !keyword_char() {
                Statement::If(IfStatement {
                    condition: cond,
                    then_body,
                    elif_branches: elifs,
                    else_body,
                })
            }

        rule elif_branch() -> (Pipeline, Vec<Statement>)
            = "else" !keyword_char() _+ "if" !keyword_char() _+ cond:pipeline() statement_sep() body:statement_list() {
                (cond, body)
            }

        rule else_branch() -> Vec<Statement>
            = "else" !keyword_char() statement_sep() body:statement_list() {
                body
            }

        rule switch_stmt() -> Statement
            = "switch" !keyword_char() _+ val:word() statement_sep()
              cases:case_clause()*
              "end" !keyword_char() {
                Statement::Switch(SwitchStatement {
                    value: val,
                    cases,
                })
            }

        rule case_clause() -> CaseClause
            = "case" !keyword_char() _+ pats:(word() ++ (_+)) statement_sep()
              body:statement_list() {
                CaseClause { patterns: pats, body }
            }

        rule for_stmt() -> Statement
            = "for" !keyword_char() _+ var:$(['a'..='z' | 'A'..='Z' | '_']['a'..='z' | 'A'..='Z' | '0'..='9' | '_']*) _+ "in" !keyword_char() _+ vals:(word() ++ (_+)) statement_sep()
              body:statement_list()
              "end" !keyword_char() {
                Statement::For(ForStatement {
                    variable: var.to_string(),
                    values: vals,
                    body,
                })
            }

        rule while_stmt() -> Statement
            = "while" !keyword_char() _+ cond:pipeline() statement_sep()
              body:statement_list()
              "end" !keyword_char() {
                Statement::While(WhileStatement {
                    condition: cond,
                    body,
                })
            }

        rule function_stmt() -> Statement
            = "function" !keyword_char() _+ name:word() opts:func_opt()* statement_sep()
              body:statement_list()
              "end" !keyword_char() {
                let mut named_args = Vec::new();
                let mut description = None;
                for opt in opts {
                    match opt {
                        FuncOpt::Args(mut a) => named_args.append(&mut a),
                        FuncOpt::Desc(d) => description = Some(d),
                    }
                }
                let func_name = name.as_single_literal().unwrap_or("").to_string();
                Statement::Function(FunctionStatement {
                    name: func_name,
                    named_args,
                    description,
                    body,
                })
            }

        rule func_opt() -> FuncOpt
            = _+ ("-a" / "--argument-names") _+ names:(ident() ++ (_+)) {
                FuncOpt::Args(names)
            }
            / _+ ("-d" / "--description") _+ w:word() {
                let desc = match &w.parts[0] {
                    WordPart::SingleQuoted(s) => s.clone(),
                    WordPart::DoubleQuoted(parts) => {
                        let mut s = String::new();
                        for p in parts {
                            if let WordPart::Literal(lit) = p {
                                s.push_str(lit);
                            }
                        }
                        s
                    }
                    WordPart::Literal(s) => s.clone(),
                    _ => "".to_string(),
                };
                FuncOpt::Desc(desc)
            }

        rule begin_stmt() -> Statement
            = "begin" !keyword_char() statement_sep()
              body:statement_list()
              "end" !keyword_char() redirs:(_* r:redirection() { r })* {
                Statement::BeginBlock(BeginBlock {
                    body,
                    redirections: redirs,
                })
            }
    }
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                match next {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    _ => out.push(next),
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn parse(input: &str) -> Result<Program, peg::error::ParseError<peg::str::LineCol>> {
    fish_grammar::program(input.trim_start_matches('\u{feff}'))
}
