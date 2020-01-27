use grafeia_core::{
    content::*,
    units::*,
    builder::ContentBuilder,
    draw::{Cache, Page},
    layout::FlexMeasure,
    Color, Display
};
use font;
use pathfinder_renderer::scene::Scene;
use pathfinder_geometry::vector::Vector2F;
use crate::view::Interactive;
use winit::event::{ElementState, VirtualKeyCode};
use vector::{PathBuilder, PathStyle, Surface};
use unicode_segmentation::UnicodeSegmentation;
use serde::{Serialize, Deserialize};
use std::fs::File;

#[derive(Serialize, Deserialize)]
pub struct App {
    storage: Storage,
    target: Target,
    document: Sequence,
    design: Design,

    #[serde(skip)]
    cache: Cache,

    #[serde(skip)]
    pages: Vec<Page>,

    #[serde(skip)]
    cursor: Option<Cursor>
}
impl App {
    pub fn build() -> Self {
        info!("App::build()");

        let mut storage = Storage::new();
        let document = ContentBuilder::new(&mut storage)
            .chapter().word("Test").finish()
            .paragraph()
                .word("The")
                .word("distilled")
                .word("spirit")
                .word("of")
                .word("Garamond")
                .finish()
            .paragraph()
                .word("The")
                .word("ffine")
                .word("fish")
                .finish()
            .finish();
        
        let target = Target {
            description: "test target".into(),
            content_box: Rect {
                left: Length::mm(10.),
                width: Length::mm(150.),
                top: Length::mm(10.),
                height: Length::mm(220.)
            },
            media_box: Rect {
                left: Length::mm(-3.),
                width: Length::mm(176.),
                top: Length::mm(-3.),
                height: Length::mm(246.)
            },
            trim_box: Rect {
                left: Length::mm(0.),
                width: Length::mm(170.),
                top: Length::mm(0.),
                height: Length::mm(240.)
            },
            page_color: Color
        };

        info!("reading font");
        let font_face = storage.insert_font_face(
            Vec::from(&include_bytes!("../../data/Cormorant-Regular.ttf")[..]).into()
        );

        info!("done reading font");

        let default = TypeDesign {
            display:   Display::Inline,
            font:           Font {
                font_face,
                size:  Length::mm(4.0)
            },
            word_space: FlexMeasure {
                height:  Length::zero(),
                shrink:  Length::mm(1.0),
                width:   Length::mm(2.0),
                stretch: Length::mm(3.0)
            },
            line_height: Length::mm(5.0)
        };

        
        let mut design = Design::new("default design".into(), default);
        design.set_type(
            storage.find_type("chapter").unwrap(),
            TypeDesign {
                display:        Display::Block,
                font:           Font {
                    font_face,
                    size:  Length::mm(8.0)
                },
                word_space: FlexMeasure {
                    height:  Length::zero(),
                    shrink:  Length::mm(2.0),
                    width:   Length::mm(4.0),
                    stretch: Length::mm(6.0)
                },
                line_height: Length::mm(10.0)
            }
        );
        design.set_type(
            storage.find_type("paragraph").unwrap(),
            TypeDesign {
                display:        Display::Paragraph(Length::mm(10.0)),
                font:           Font {
                    font_face,
                    size:  Length::mm(4.0)
                },
                word_space: FlexMeasure {
                    height:  Length::zero(),
                    shrink:  Length::mm(1.0),
                    width:   Length::mm(2.0),
                    stretch: Length::mm(3.0)
                },
                line_height: Length::mm(5.0)
            }
        );

        let mut cache = Cache::new();
        info!("rendering document");
        let pages = cache.render(&storage, &target, &document, &design);
        info!("App ready");

        App {
            cache,
            storage,
            target,
            document,
            design,
            pages,
            cursor: None
        }
    }
    pub fn store(&self) {
        bincode::serialize_into(File::create("app.data").unwrap(), self).unwrap()
    }
    pub fn load() -> Option<Self> {
        let file = File::open("app.data").ok()?;
        let mut app: Self = bincode::deserialize_from(file).ok()?;
        app.render();
        Some(app)
    }
    fn render(&mut self) {
        self.pages = self.cache.render(&self.storage, &self.target, &self.document, &self.design);
    }
    fn set_cursor_to(&mut self, tag: Tag, text_pos: usize) {
        self.cursor = self.cache.get_position_on_page(&self.storage, &self.design, &self.document, &self.pages[0], tag, text_pos)
            .map(|(page_pos, type_key)| Cursor {
                tag,
                text_pos,
                page_pos,
                type_key
            });
    }
    fn text_op(&mut self, op: TextOp) -> bool {
        let cursor = match self.cursor {
            Some(cursor) => cursor,
            _ => return false
        };

        match self.document.find(cursor.tag) {
            Some((_, &Item::Word(word_key))) => {
                let text = &self.storage.get_word(word_key).text;
                let n = cursor.text_pos;

                let (new_text, text_pos): (String, usize) = match op {
                    TextOp::DeleteLeft if n > 0 => {
                        let new_pos = text[.. n].grapheme_indices(true).rev().next()
                            .map(|(n, _)| n).unwrap_or(0);
                        let new_text = format!("{}{}", &text[.. new_pos], &text[n ..]);
                        (new_text, new_pos)
                    }
                    TextOp::DeleteRight if n < text.len() => {
                        let new_pos = text[n ..].grapheme_indices(true).nth(1)
                            .map(|(m, _)| n + m).unwrap_or(text.len());
                        let new_text = format!("{}{}", &text[.. n], &text[new_pos ..]);
                        (new_text, new_pos)
                    }
                    TextOp::Insert(c) => {
                        let new_text = format!("{}{}{}", &text[.. cursor.text_pos], c, &text[cursor.text_pos ..]);
                        (new_text, cursor.text_pos + c.len_utf8())
                    }
                    _ => return false
                };

                let new_item = Item::Word(self.storage.insert_word(&new_text));
                self.document.replace(cursor.tag, new_item);

                self.render();

                self.cursor = self.cache.get_position_on_page(&self.storage, &self.design, &self.document, &self.pages[0], cursor.tag, text_pos)
                    .map(|(page_pos, type_key)| Cursor {
                        tag: cursor.tag,
                        text_pos,
                        page_pos,
                        type_key
                    });
                true
            }
            _ => false
        }
    }
    fn cursor_op(&mut self, op: CursorOp) -> bool {
        let cursor = match self.cursor {
            Some(cursor) => cursor,
            _ => return false
        };

        match self.document.find(cursor.tag) {
            Some((_, &Item::Word(word_key))) => {
                let text = &self.storage.get_word(word_key).text;
                let pos = match op {
                    CursorOp::GraphemeRight => {
                        text[cursor.text_pos ..].grapheme_indices(true).nth(1)
                            .map(|(n, _)| cursor.text_pos+n).unwrap_or(text.len())
                    }
                    CursorOp::GraphemeLeft => {
                        text[.. cursor.text_pos].grapheme_indices(true).rev().next()
                            .map(|(n, _)| n).unwrap_or(0)
                    }
                };
                self.set_cursor_to(cursor.tag, pos);
                return true;
            }
            _ => false
        }
    }
}

enum TextOp {
    Insert(char),
    DeleteLeft,
    DeleteRight
}
enum CursorOp {
    GraphemeLeft,
    GraphemeRight,
}

#[derive(PartialEq, Copy, Clone)]
struct Cursor {
    tag: Tag,   // which item
    text_pos: usize, // which byte in the item (if applicable)
    page_pos: Vector2F,
    type_key: TypeKey
}

impl Interactive for App {
    fn title(&self) -> String {
        "γραφείο".into()
    }
    fn scene(&mut self) -> Scene {
        let mut scene = self.pages[0].scene().clone();
        if let Some(ref cursor) = self.cursor {
            let type_design = self.design.get_type_or_default(cursor.type_key);
            let style = scene.build_style(PathStyle {
                fill: None,
                stroke: Some(((0,0,200,255), 0.1 * type_design.font.size.value))
            });
            let mut pb = PathBuilder::new();
            pb.move_to(cursor.page_pos);
            pb.line_to(cursor.page_pos - Vector2F::new(0.0, type_design.font.size.value));
            
            scene.draw_path(pb.into_outline(), &style);
        }

        scene
    }
    fn mouse_input(&mut self, pos: Vector2F, state: ElementState) -> bool {
        info!("mouse input at {:?}, state = {:?}", pos, state);
        let old_cursor = self.cursor.take();

        dbg!(pos, state);
        if let Some((tag, word_pos)) = self.pages[0].find(pos) {
            let item = self.document.find(tag);
            println!("clicked on {:?}", item);
            let offset = pos.x() - word_pos.x();

            self.cursor = self.cache.find(&self.storage, &self.design, &self.document, offset, tag)
                .map(|(word_offset, n, typ)| Cursor {
                    tag,
                    page_pos: word_offset + word_pos,
                    text_pos: n,
                    type_key: typ
                });
        }

        self.cursor != old_cursor
    }

    fn keyboard_input(&mut self, state: ElementState, keycode: VirtualKeyCode) -> bool {
        info!("keyboard input keycode = {:?}, state = {:?}", keycode, state);
        match (state, keycode) {
            (ElementState::Pressed, VirtualKeyCode::Right) => self.cursor_op(CursorOp::GraphemeRight),
            (ElementState::Pressed, VirtualKeyCode::Left) => self.cursor_op(CursorOp::GraphemeLeft),
            (ElementState::Pressed, VirtualKeyCode::Back) => self.text_op(TextOp::DeleteLeft),
            (ElementState::Pressed, VirtualKeyCode::Delete) => self.text_op(TextOp::DeleteRight),
            _ => false
        }
    }

    fn char_input(&mut self, c: char) -> bool {
        match c {
            // backspace
            '\u{8}' => return false,
            ' ' => return false,
            _ => self.text_op(TextOp::Insert(c))
        }
    }
    fn exit(&mut self) {
        self.store()
    }
}
