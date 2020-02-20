use crate::{Storage, TypeId, Item, Type, Document, Object};

pub struct ContentBuilder {
    document: Document,
    document_key: TypeId,
    para_key: TypeId,
    chapter_key: TypeId,
    items: Vec<Item>
}
impl ContentBuilder {
    pub fn new() -> Self {
        let storage = Storage::new();
        let mut document = Document::new(storage);

        ContentBuilder {
            document_key: document.create_type(
                "document",
                Type::new("The Document")
            ),
            para_key: document.create_type(
                "paragraph",
                Type::new("A Paragraph")
            ),
            chapter_key: document.create_type(
                "chapter",
                Type::new("A Chapter")
            ),
            document,
            items: vec![]
        }
    }
    pub fn chapter(self) -> TextBuilder {
        TextBuilder {
            typ: self.chapter_key,
            nodes: vec![],
            parent: self
        }
    }
    pub fn paragraph(self) -> TextBuilder {
        TextBuilder {
            typ: self.para_key,
            nodes: vec![],
            parent: self
        }
    }
    pub fn object(mut self, object: Object) -> Self {
        let key = self.document.create_object(object);
        self.items.push(Item::Object(key));
        self
    }
    pub fn finish(mut self) -> Document {
        let root = self.document.creat_seq_with_items(self.document_key, self.items.into_iter());
        self.document.set_root(root);
        self.document
    }
}

pub struct TextBuilder {
    parent: ContentBuilder,
    typ:    TypeId,
    nodes:  Vec<Item>
}
impl TextBuilder {
    pub fn word(mut self, w: &str) -> Self {
        let word = self.parent.document.create_word(w);
        self.nodes.push(Item::Word(word));
        self
    }

    pub fn text(mut self, text: &str) -> Self {
        for w in text.split_ascii_whitespace() {
            let word = self.parent.document.create_word(w);
            self.nodes.push(Item::Word(word));
        }
        self
    }

    pub fn object(mut self, object: Object) -> Self {
        let key = self.parent.document.create_object(object);
        self.nodes.push(Item::Object(key));
        self
    }

    pub fn finish(mut self) -> ContentBuilder {
        let key = self.parent.document.creat_seq_with_items(self.typ, self.nodes.into_iter());
        self.parent.items.push(Item::Sequence(key));
        self.parent
    }
}