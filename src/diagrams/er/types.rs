#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cardinality {
    ZeroOrOne,
    ZeroOrMore,
    OneOrMore,
    OnlyOne,
    MdParent,
}

impl Cardinality {
    pub fn as_lower_str(self) -> &'static str {
        match self {
            Cardinality::ZeroOrOne => "zero_or_one",
            Cardinality::ZeroOrMore => "zero_or_more",
            Cardinality::OneOrMore => "one_or_more",
            Cardinality::OnlyOne => "only_one",
            Cardinality::MdParent => "md_parent",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Identification {
    NonIdentifying,
    Identifying,
}

#[derive(Debug, Clone)]
pub struct ErAttribute {
    pub attr_type: String,
    pub name: String,
    pub keys: Vec<String>,
    pub comment: String,
}

// What the parser hands over for one attribute; keys and comment are present only when written.
#[derive(Debug, Clone)]
pub struct ParsedAttribute {
    pub attr_type: String,
    pub name: String,
    pub keys: Option<Vec<String>>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ErEntity {
    pub id: String,
    pub label: String,
    pub alias: String,
    pub attributes: Vec<ErAttribute>,
    pub css_classes: String,
    pub css_styles: Vec<String>,
}

// card_b sits at entity_a's end of the line and card_a at entity_b's, as in Mermaid.
#[derive(Debug, Clone, Copy)]
pub struct RelSpec {
    pub card_a: Cardinality,
    pub rel_type: Identification,
    pub card_b: Cardinality,
}

// entity_a and entity_b hold an entity's id, or a subgraph's id when the name was a subgraph at the time.
#[derive(Debug, Clone)]
pub struct ErRelationship {
    pub entity_a: String,
    pub role_a: String,
    pub entity_b: String,
    pub rel_spec: RelSpec,
}

#[derive(Debug, Clone)]
pub struct ErClassDef {
    pub id: String,
    pub styles: Vec<String>,
    pub text_styles: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ErSubgraph {
    pub id: String,
    pub nodes: Vec<String>,
    pub title: String,
    pub classes: Vec<String>,
    pub css_styles: Vec<String>,
    pub dir: Option<String>,
    pub label_type: &'static str,
}

#[derive(Debug, Clone)]
pub enum DocItem {
    Str(String),
    Dir(String),
}
