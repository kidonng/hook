use crate::ast::*;

#[derive(Debug)]
enum CommandItem {
    Arg(Word),
    Redir(Redirection),
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
                stmts.into_iter().flatten().filter(|s| match s {
                    Statement::Comment(_) => true,
                    Statement::Pipeline(p) => !p.commands.is_empty(),
                    _ => true,
                }).collect()
            }

        rule statement_sep()
            = (_* ['\n' | ';'])+ _*

        rule _() = [' ' | '\t']

        rule ident() -> String
            = s:$(['a'..='z' | 'A'..='Z' | '_']['a'..='z' | 'A'..='Z' | '0'..='9' | '_']*) { s.to_string() }

        rule keyword_char() = ['a'..='z' | 'A'..='Z' | '0'..='9' | '_']

        rule reserved_keyword()
            = ("if" / "else" / "switch" / "case" / "for" / "while" / "function" / "begin" / "end") !keyword_char()

        rule block_terminator()
            = ("end" / "else" / "case") !keyword_char()
            / "}"
        pub rule statement() -> Vec<Statement>
            = !block_terminator() s:inner_statement() { s }

        rule inner_statement() -> Vec<Statement>
            = s:comment() { vec![s] }
            / s:return_stmt() { vec![s] }
            / s:break_stmt() { vec![s] }
            / s:continue_stmt() { vec![s] }
            / s:if_stmt() { vec![s] }
            / s:switch_stmt() { vec![s] }
            / s:for_stmt() { vec![s] }
            / s:while_stmt() { vec![s] }
            / s:function_stmt() { vec![s] }
            / s:begin_stmt() { vec![s] }
            / s:compound_block_stmt() { vec![s] }
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
        rule pipeline_stmt() -> Vec<Statement>
            = chain:pipeline_chain() {
                chain.into_iter().map(Statement::Pipeline).collect()
            }

        rule pipeline_chain() -> Vec<Pipeline>
            = head:pipeline() tail:(cont_space() comb:combinator_op() cont_space() p:pipeline() { (comb, p) })* {
                let mut list = vec![head];
                for (comb, mut p) in tail {
                    p.combinator = comb;
                    list.push(p);
                }
                list
            }

        rule combinator_op() -> Combinator
            = "and" !keyword_char() { Combinator::And }
            / "or" !keyword_char() { Combinator::Or }
            / "&&" { Combinator::And }
            / "||" { Combinator::Or }

        rule cont_space()
            = (_ / ("\\" "\n") / ("#" [^'\n']* "\n") / "\n")*

        rule pipe_op() -> bool
            = ("&|" / "|&") { true }
            / "|" { false }

        rule pipe_sep() -> bool
            = _* p:pipe_op() cont_space() { p }

        rule negate() -> bool
            = ("not" !keyword_char() _+ / "!" _*) { true }

        rule pipeline() -> Pipeline
            = !reserved_keyword() comb:combinator_prefix()? _* negate:negate()? head:command() tail:(sep:pipe_sep() cmd:command() { (sep, cmd) })* bg:(_* "&" !['&' | '>'])? {
                let mut commands = vec![head];
                let mut pipe_operators = Vec::new();
                for (is_merged, next_cmd) in tail {
                    pipe_operators.push(if is_merged {
                        PipeOperator::StdoutAndStderr
                    } else {
                        PipeOperator::Stdout
                    });
                    commands.push(next_cmd);
                }
                if let Some(true) = negate.map(|_| true) {
                    if let Some(first) = commands.first_mut() {
                        first.negate = true;
                    }
                }
                Pipeline {
                    commands,
                    pipe_operators,
                    combinator: comb.unwrap_or(Combinator::None),
                    background: bg.is_some(),
                }
            }

        rule combinator_prefix() -> Combinator
            = "and" !keyword_char() _+ { Combinator::And }
            / "or" !keyword_char() _+ { Combinator::Or }
            / "&&" _* { Combinator::And }
            / "||" _* { Combinator::Or }

        rule line_cont()
            = "\\" _* "\n" (_* "#" [^'\n']* "\n")* _*

        rule cmd_arg_sep()
            = (_ / line_cont())+

        rule command() -> Command
            = items:(command_item() ++ cmd_arg_sep()) {
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
            = ">>?" { RedirectMode::NoClobberAppend }
            / ">>" { RedirectMode::Append }
            / ">&" { RedirectMode::DupOutput }
            / "<&" { RedirectMode::DupInput }
            / "<?" { RedirectMode::SafeInput }
            / ">?" { RedirectMode::NoClobberOutput }
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
            / dollar_command_subst()
            / s:$(( [^'\"' | '$' | '\\'] / ("\\" [_]) )+) {
                WordPart::Literal(unescape(s))
            }

        rule variable_ref() -> WordPart
            = "$" inner:variable_ref() slices:slice()* {
                if let WordPart::Variable(vref) = inner {
                    WordPart::Variable(VariableRef {
                        target: VariableTarget::Indirect(Box::new(vref)),
                        slices,
                    })
                } else {
                    unreachable!()
                }
            }
            / "$" name:$(['a'..='z' | 'A'..='Z' | '_']['a'..='z' | 'A'..='Z' | '0'..='9' | '_']*) slices:slice()* {
                WordPart::Variable(VariableRef {
                    target: VariableTarget::Named(name.to_string()),
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
            / v:variable_ref() {
                if let WordPart::Variable(vref) = v {
                    SliceIndex::Variable(vref)
                } else {
                    unreachable!()
                }
            }
        rule dollar_command_subst() -> WordPart
            = "$(" _* stmts:statement_list() _* ")" slices:slice()* {
                WordPart::CommandSubst { statements: stmts, slices }
            }

        rule command_subst() -> WordPart
            = dollar_command_subst()
            / "(" _* stmts:statement_list() _* ")" slices:slice()* {
                WordPart::CommandSubst { statements: stmts, slices }
            }
        rule brace_expansion() -> WordPart
            = "{" _* words:(word() ** (_* "," _*)) _* "}" {
                WordPart::BraceExpansion(words)
            }

        rule if_stmt() -> Statement
            = "if" !keyword_char() _+ cond:pipeline_chain() statement_sep()
              then_body:statement_list()
              elifs:elif_branch()*
              else_body:else_branch()?
              "end" !keyword_char() redirs:(_* r:redirection() { r })* {
                Statement::If(IfStatement {
                    condition: cond,
                    then_body,
                    elif_branches: elifs,
                    else_body,
                    redirections: redirs,
                })
            }

        rule elif_branch() -> (Vec<Pipeline>, Vec<Statement>)
            = "else" !keyword_char() _+ "if" !keyword_char() _+ cond:pipeline_chain() statement_sep() body:statement_list() {
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
              "end" !keyword_char() redirs:(_* r:redirection() { r })* {
                Statement::For(ForStatement {
                    variable: var.to_string(),
                    values: vals,
                    body,
                    redirections: redirs,
                })
            }

        rule while_stmt() -> Statement
            = "while" !keyword_char() _+ cond:pipeline_chain() statement_sep()
              body:statement_list()
              "end" !keyword_char() redirs:(_* r:redirection() { r })* {
                Statement::While(WhileStatement {
                    condition: cond,
                    body,
                    redirections: redirs,
                })
            }

        rule function_stmt() -> Statement
            = "function" !keyword_char() _+ name:word() options:(_+ w:word() { w })* statement_sep()
              body:statement_list()
              "end" !keyword_char() {
                Statement::Function(FunctionStatement {
                    name,
                    options,
                    body,
                })
            }
        rule begin_stmt() -> Statement
            = comb:combinator_prefix()? _* "begin" !keyword_char() statement_sep()
              body:statement_list()
              "end" !keyword_char() redirs:(_* r:redirection() { r })* {
                Statement::BeginBlock(BeginBlock {
                    combinator: comb.unwrap_or(Combinator::None),
                    body,
                    redirections: redirs,
                })
            }
        rule compound_block_stmt() -> Statement
            = comb:combinator_prefix()? _* "{" (_* ['\n' | ';'])* _*
              body:statement_list()
              _* "}" redirs:(_* r:redirection() { r })* {
                Statement::BeginBlock(BeginBlock {
                    combinator: comb.unwrap_or(Combinator::None),
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
