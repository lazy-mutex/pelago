#[derive(Debug, Clone)]
pub struct StateNote {
    pub position: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone)]
pub enum Description {
    Single(String),
    List(Vec<String>),
}

impl Description {
    pub fn is_truthy(&self) -> bool {
        match self {
            Description::Single(s) => !s.is_empty(),
            Description::List(_) => true,
        }
    }

    pub fn into_vec(self) -> Vec<String> {
        match self {
            Description::Single(s) => vec![s],
            Description::List(v) => v,
        }
    }

    pub fn js_len(&self) -> usize {
        match self {
            Description::Single(s) => s.encode_utf16().count(),
            Description::List(v) => v.len(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StateStmt {
    pub id: String,
    pub kind: Option<String>,
    pub description: Option<Description>,
    pub descriptions: Vec<String>,
    pub doc: Option<Vec<Stmt>>,
    pub note: Option<StateNote>,
    pub start: Option<bool>,
    pub classes: Vec<String>,
    pub styles: Vec<String>,
    pub text_styles: Vec<String>,
}

impl StateStmt {
    pub fn new(id: String, kind: Option<String>) -> Self {
        StateStmt {
            id,
            kind,
            description: None,
            descriptions: Vec::new(),
            doc: None,
            note: None,
            start: None,
            classes: Vec::new(),
            styles: Vec::new(),
            text_styles: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RelationStmt {
    pub state1: StateStmt,
    pub state2: StateStmt,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ClassDefStmt {
    pub id: String,
    pub classes: String,
}

#[derive(Debug, Clone)]
pub struct StyleStmt {
    pub id: String,
    pub style_class: String,
}

#[derive(Debug, Clone)]
pub struct ApplyClassStmt {
    pub id: String,
    pub style_class: String,
}

#[derive(Debug, Clone)]
pub struct DirectionStmt {
    pub value: String,
}

#[derive(Debug, Clone)]
pub enum ClickTarget {
    State(StateStmt),
    Id(String),
}

#[derive(Debug, Clone)]
pub struct ClickStmt {
    pub id: ClickTarget,
    pub url: String,
    pub tooltip: String,
}

// Mermaid's grammar also leaves the text of statements that have no effect in a document
// (`hide empty description`, `scale`, a bare `state name`, a floating note); those are `Text`.
#[derive(Debug, Clone)]
pub enum Stmt {
    Text(String),
    State(StateStmt),
    Relation(RelationStmt),
    ClassDef(ClassDefStmt),
    Style(StyleStmt),
    ApplyClass(ApplyClassStmt),
    Dir(DirectionStmt),
    Click(ClickStmt),
}

#[derive(Debug, Clone)]
pub struct StateRelation {
    pub id1: String,
    pub id2: String,
    pub relation_title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StateClassDef {
    pub id: String,
    pub styles: Vec<String>,
    pub text_styles: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct StateLink {
    pub url: String,
    pub tooltip: String,
}
