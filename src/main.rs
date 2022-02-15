use std::rc::Rc;

/// Type definitions for formatters
///
/// The main difference between this and Rome formatter is that we
/// pre-emptively calculate the flattened version. This may not be
/// better because Haskell is a lazy language and therefore it
/// might not do it pre-emptively in the Haskell version

#[derive(Debug, Clone)]
enum FormatElement {
    Empty,
    List(Vec<Rc<FormatElement>>),
    Indent(usize, Rc<FormatElement>),
    Token(String),
    LineOrSpace,
    LineOrEmpty,
    Union(Rc<FormatElement>, Rc<FormatElement>),
}

enum FittedDocument {
    Empty,
    Text(String, Box<FittedDocument>),
    Line(usize, Box<FittedDocument>),
}

/// Helper functions for creating documents

fn empty() -> FormatElement {
    FormatElement::Empty
}

fn concat(docs: Vec<Rc<FormatElement>>) -> FormatElement {
    FormatElement::List(docs)
}

fn nest(indent: usize, x: FormatElement) -> FormatElement {
    FormatElement::Indent(indent, Rc::new(x))
}

fn text(s: String) -> FormatElement {
    FormatElement::Token(s)
}

fn literal(s: &'static str) -> FormatElement {
    FormatElement::Token(s.to_string())
}

fn line_or_space() -> FormatElement {
    FormatElement::LineOrSpace
}

fn line_or_empty() -> FormatElement {
    FormatElement::LineOrEmpty
}

fn group(x: FormatElement) -> FormatElement {
    let x = Rc::new(x);
    FormatElement::Union(flatten(x.clone()), x)
}

fn flatten(x: Rc<FormatElement>) -> Rc<FormatElement> {
    match &*x {
        FormatElement::Empty | FormatElement::Token(_) => x.clone(),
        FormatElement::List(docs) => Rc::new(FormatElement::List(
            docs.iter().map(|d| flatten(d.clone())).collect(),
        )),
        FormatElement::Indent(offset, x) => {
            Rc::new(FormatElement::Indent(*offset, flatten(x.clone())))
        }
        FormatElement::LineOrSpace => Rc::new(text(" ".to_string())),
        FormatElement::LineOrEmpty => Rc::new(empty()),
        FormatElement::Union(x, _) => flatten(x.clone()),
    }
}

fn print_fitted_document(x: FittedDocument) -> String {
    match x {
        FittedDocument::Empty => String::new(),
        FittedDocument::Text(s, x) => {
            format!("{}{}", s, print_fitted_document(*x))
        }
        FittedDocument::Line(offset, x) => {
            let indent = " ".repeat(offset);
            format!("\n{}{}", indent, print_fitted_document(*x))
        }
    }
}

fn fit_document(width: usize, chars_placed: usize, x: Rc<FormatElement>) -> FittedDocument {
    fit_documents(width, chars_placed, vec![(0, x)])
}

fn fit_documents(
    width: usize,
    chars_placed: usize,
    mut documents: Vec<(usize, Rc<FormatElement>)>,
) -> FittedDocument {
    if let Some((indent, doc)) = documents.pop() {
        match &*doc {
            FormatElement::Empty => fit_documents(width, chars_placed, documents),
            FormatElement::List(docs) => {
                for doc in docs.iter().rev() {
                    documents.push((indent, doc.clone()));
                }

                fit_documents(width, chars_placed, documents)
            }
            FormatElement::Indent(offset, x) => {
                documents.push((indent + offset, x.clone()));
                fit_documents(width, chars_placed, documents)
            }
            FormatElement::Token(s) => {
                let length = s.len();
                FittedDocument::Text(
                    s.clone(),
                    Box::new(fit_documents(width, chars_placed + length, documents)),
                )
            }
            FormatElement::LineOrSpace | FormatElement::LineOrEmpty => {
                FittedDocument::Line(indent, Box::new(fit_documents(width, indent, documents)))
            }
            FormatElement::Union(x, y) => {
                // NOTE: This is slow right now because we're cloning a vector of documents
                // We'll make this less slow in the future with immutable vectors
                let mut left_documents = documents.clone();
                left_documents.push((indent, x.clone()));
                let mut right_documents = documents;
                right_documents.push((indent, y.clone()));
                pick_better_fit(
                    width,
                    chars_placed,
                    move || fit_documents(width, chars_placed, left_documents),
                    move || fit_documents(width, chars_placed, right_documents),
                )
            }
        }
    } else {
        FittedDocument::Empty
    }
}

fn pick_better_fit(
    width: usize,
    chars_left: usize,
    x_thunk: impl FnOnce() -> FittedDocument,
    y_thunk: impl FnOnce() -> FittedDocument,
) -> FittedDocument {
    let x = x_thunk();
    if x.fits(width - chars_left) {
        x
    } else {
        y_thunk()
    }
}

impl FittedDocument {
    fn fits(&self, width: usize) -> bool {
        if width == 0 {
            false
        } else {
            match self {
                FittedDocument::Empty => true,
                FittedDocument::Text(s, x) => x.fits(width.saturating_sub(s.len())),
                FittedDocument::Line(_i, _x) => true,
            }
        }
    }
}

fn pretty(width: usize, x: Rc<FormatElement>) -> String {
    print_fitted_document(fit_document(width, 0, x))
}

enum JsonValue {
    Array(Vec<JsonValue>),
    Number(i64),
    String(String),
    Object(Vec<(String, JsonValue)>),
}

impl From<JsonValue> for FormatElement {
    fn from(value: JsonValue) -> Self {
        match value {
            JsonValue::Array(entries) => {
                let mut entry_documents = Vec::new();
                let entries_len = entries.len();

                for (idx, entry) in entries.into_iter().enumerate() {
                    if idx == 0 {
                        entry_documents.push(Rc::new(line_or_empty()));
                    }
                    entry_documents.push(Rc::new(entry.into()));

                    if idx != entries_len - 1 {
                        entry_documents.push(Rc::new(literal(",")));
                        entry_documents.push(Rc::new(line_or_space()));
                    }
                }

                let nested_entries =
                    FormatElement::Indent(2, Rc::new(FormatElement::List(entry_documents)));

                group(FormatElement::List(vec![
                    Rc::new(FormatElement::Token("[".to_string())),
                    Rc::new(nested_entries),
                    Rc::new(line_or_empty()),
                    Rc::new(FormatElement::Token("]".to_string())),
                ]))
            }
            JsonValue::Number(i) => FormatElement::Token(i.to_string()),
            JsonValue::String(s) => FormatElement::Token(format!("\"{}\"", s)),
            JsonValue::Object(fields) => {
                let mut object_documents = Vec::new();
                let fields_len = fields.len();
                for (idx, (field_name, field_value)) in fields.into_iter().enumerate() {
                    if idx == 0 {
                        object_documents.push(Rc::new(line_or_space()));
                    }

                    object_documents.push(Rc::new(text(field_name)));
                    object_documents.push(Rc::new(literal(": ")));
                    object_documents.push(Rc::new(field_value.into()));

                    if idx != fields_len - 1 {
                        object_documents.push(Rc::new(text(",".to_string())));
                        object_documents.push(Rc::new(line_or_space()))
                    }
                }

                group(FormatElement::List(vec![
                    Rc::new(literal("{")),
                    Rc::new(nest(2, concat(object_documents))),
                    Rc::new(line_or_space()),
                    Rc::new(literal("}")),
                ]))
            }
        }
    }
}

fn main() {
    let document: Rc<FormatElement> = Rc::new(
        JsonValue::Array(vec![
            JsonValue::Object(vec![(
                "Wong Kar Wai".to_string(),
                JsonValue::Array(vec![
                    JsonValue::String("Chungking Express".to_string()),
                    JsonValue::String("In the Mood for Love".to_string()),
                    JsonValue::String("Happy Together".to_string()),
                ]),
            )]),
            JsonValue::Object(vec![(
                "Lucrecia Martel".to_string(),
                JsonValue::Array(vec![
                    JsonValue::String("La Cienaga".to_string()),
                    JsonValue::String("A Headless Woman".to_string()),
                    JsonValue::String("Zama".to_string()),
                ]),
            )]),
            JsonValue::Object(vec![(
                "Olivier Assayas".to_string(),
                JsonValue::Array(vec![
                    JsonValue::String("Summer Hours".to_string()),
                    JsonValue::String("Irma Vep".to_string()),
                    JsonValue::String("Clouds of Sils Maria".to_string()),
                ]),
            )]),
        ])
        .into(),
    );

    println!("{}", pretty(50, document.clone()));
    println!("{}", pretty(80, document.clone()));
    println!("{}", pretty(100, document.clone()));
    println!("{}", pretty(150, document.clone()));
}

#[test]
fn raw_test() {
    let doc = group(FormatElement::List(vec![
        Rc::new(FormatElement::Token(
            "[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, ".to_string(),
        )),
        Rc::new(FormatElement::LineOrSpace),
        Rc::new(FormatElement::Token(
            "11, 12, 13, 14, 15, 16, 17, 18, 19, 20]".to_string(),
        )),
    ]));

    assert_eq!(
        "[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20]",
        pretty(200, doc.clone())
    );

    assert_eq!(
        "[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, \n11, 12, 13, 14, 15, 16, 17, 18, 19, 20]",
        pretty(50, doc)
    );
}
