#import "lib.typ": mermaid

= Introduction

#mermaid(
  "
flowchart TD
  A[Mermaid text] --> B{Flowchart?}
  B -- Yes --> C[Pele]
  B -- No --> D[Mermaid]
  C --> E([SVG])
  D --> E
",
)


#mermaid(
  "
swimlane-beta LR
  subgraph Applicant
    apply[Submit\napplication]
    sign[Sign\nagreement]
  end

  subgraph Reviewer
    screen[Screen\napplication]
    decide{Approved?}
  end

  subgraph System
    create[Create\naccount]
    notify[Send\nwelcome\nemail]
  end

  apply -->|Application\nreceived| screen
  screen --> decide
  decide -->|Approved| create --> notify --> sign
  decide -->|Needs\nchanges| apply
",
)

#mermaid(```
railroad-beta
title Expression Grammar

expression = sequence(
    nonterminal("term"),
    zeroOrMore(sequence(
        choice(terminal("+"), terminal("-")),
        nonterminal("term")
    ))
) ;
term = sequence(
    nonterminal("factor"),
    zeroOrMore(sequence(
        choice(terminal("*"), terminal("/")),
        nonterminal("factor")
    ))
) ;
factor = choice(
    nonterminal("number"),
    sequence(terminal("("), nonterminal("expression"), terminal(")"))
) ;
number = oneOrMore(nonterminal("digit")) ;
digit = choice(terminal("0"), terminal("1"), terminal("2"), terminal("3"), terminal("4"), terminal("5"), terminal("6"), terminal("7"), terminal("8"), terminal("9")) ;
```)

