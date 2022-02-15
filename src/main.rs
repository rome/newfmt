use std::rc::Rc;

enum JsonValue {
    Array(Vec<JsonValue>),
    Number(i64),
    String(String),
    Object(Vec<(String, JsonValue)>),
}

impl Into<Document> for JsonValue {
    fn into(self) -> Document {
        match self {
            JsonValue::Array(entries) => {
                let mut entry_documents = Vec::new();
                let entries_len = entries.len();

                for (idx, entry) in entries.into_iter().enumerate() {
                    entry_documents.push(Rc::new(entry.into()));

                    if idx != entries_len - 1 {
                        entry_documents.push(Rc::new(literal(",")));
                        entry_documents.push(Rc::new(line_or_space()));
                    }
                }

                let nested_entries = Document::Nest(2, Rc::new(Document::Concat(entry_documents)));

                group(Document::Concat(vec![
                    Rc::new(Document::Text("[".to_string())),
                    Rc::new(line_or_empty()),
                    Rc::new(nested_entries),
                    Rc::new(line_or_empty()),
                    Rc::new(Document::Text("]".to_string())),
                ]))
            }
            JsonValue::Number(i) => Document::Text(i.to_string()),
            JsonValue::String(s) => Document::Text(format!("\"{}\"", s)),
            JsonValue::Object(fields) => {
                let mut object_documents = Vec::new();
                let fields_len = fields.len();
                for (idx, (field_name, field_value)) in fields.into_iter().enumerate() {
                    let mut field_documents = Vec::new();
                    field_documents.push(Rc::new(text(field_name)));
                    field_documents.push(Rc::new(literal(": ")));
                    field_documents.push(Rc::new(field_value.into()));

                    object_documents.push(Rc::new(group(Document::Concat(field_documents))));

                    if idx != fields_len - 1 {
                        object_documents.push(Rc::new(text(",".to_string())));
                        object_documents.push(Rc::new(line_or_space()))
                    }
                }

                group(Document::Concat(vec![
                    Rc::new(literal("{")),
                    Rc::new(line_or_empty()),
                    Rc::new(nest(2, concat(object_documents))),
                    Rc::new(line_or_empty()),
                    Rc::new(literal("}")),
                ]))
            }
        }
    }
}

#[derive(Debug, Clone)]
enum Document {
    Empty,
    Concat(Vec<Rc<Document>>),
    Nest(usize, Rc<Document>),
    Text(String),
    LineOrSpace,
    LineOrEmpty,
    Union(Rc<Document>, Rc<Document>),
}

enum FittedDocument {
    Empty,
    Text(String, Box<FittedDocument>),
    Line(usize, Box<FittedDocument>),
}

fn empty() -> Document {
    Document::Empty
}

fn concat(docs: Vec<Rc<Document>>) -> Document {
    Document::Concat(docs)
}

fn nest(indent: usize, x: Document) -> Document {
    Document::Nest(indent, Rc::new(x))
}

fn text(s: String) -> Document {
    Document::Text(s)
}

fn literal(s: &'static str) -> Document {
    Document::Text(s.to_string())
}

fn line_or_space() -> Document {
    Document::LineOrSpace
}

fn line_or_empty() -> Document {
    Document::LineOrEmpty
}

fn group(x: Document) -> Document {
    let x = Rc::new(x);
    Document::Union(flatten(x.clone()), x.clone())
}

fn flatten(x: Rc<Document>) -> Rc<Document> {
    match &*x {
        Document::Empty | Document::Text(_) => x.clone(),
        Document::Concat(docs) => Rc::new(Document::Concat(
            docs.iter().map(|d| flatten(d.clone())).collect(),
        )),
        Document::Nest(offset, x) => Rc::new(Document::Nest(*offset, flatten(x.clone()))),
        Document::LineOrSpace => Rc::new(text(" ".to_string())),
        Document::LineOrEmpty => Rc::new(empty()),
        Document::Union(x, _) => flatten(x.clone()),
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

fn fit_document(width: usize, chars_placed: usize, x: Document) -> FittedDocument {
    fit_documents(width, chars_placed, vec![(0, Rc::new(x))])
}

fn fit_documents(
    width: usize,
    chars_placed: usize,
    mut documents: Vec<(usize, Rc<Document>)>,
) -> FittedDocument {
    if let Some((indent, doc)) = documents.pop() {
        match &*doc {
            Document::Empty => fit_documents(width, chars_placed, documents),
            Document::Concat(docs) => {
                for doc in docs.iter().rev() {
                    documents.push((indent, doc.clone()));
                }

                fit_documents(width, chars_placed, documents)
            }
            Document::Nest(offset, x) => {
                documents.push((indent + offset, x.clone()));
                fit_documents(width, chars_placed, documents)
            }
            Document::Text(s) => {
                let length = s.len();
                FittedDocument::Text(
                    s.clone(),
                    Box::new(fit_documents(width, chars_placed + length, documents)),
                )
            }
            Document::LineOrSpace | Document::LineOrEmpty => {
                FittedDocument::Line(indent, Box::new(fit_documents(width, indent, documents)))
            }
            Document::Union(x, y) => {
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

fn pretty(width: usize, x: Document) -> String {
    print_fitted_document(fit_document(width, 0, x))
}

fn main() {
    let document: Document = JsonValue::Array(vec![
        JsonValue::Number(10),
        JsonValue::Number(100),
        JsonValue::String("The Quick Brown Fox Jumps Over The Lazy Dog".to_string()),
        JsonValue::Array(vec![JsonValue::Object(vec![
            ("foo".to_string(), JsonValue::Number(20)),
            ("bar".to_string(), JsonValue::Number(2000000)),
            ("baz".to_string(), JsonValue::Number(2000000000000)),
            ("fan".to_string(), JsonValue::Number(2000000000000)),
            ("fab".to_string(), JsonValue::Number(2000000000000)),
        ])]),
    ])
    .into();

    println!("{:#?}", document);
    println!("{}", pretty(50, document,));
}

#[test]
fn raw_test() {
    let doc = group(Document::Concat(vec![
        Rc::new(Document::Text(
            "[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, ".to_string(),
        )),
        Rc::new(Document::LineOrSpace),
        Rc::new(Document::Text(
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
