use grafeia_core::{
    *,
    draw::{Cache, Page, RenderedWord},
};
use pathfinder_renderer::scene::Scene;
use pathfinder_geometry::{
    vector::Vector2F,
    rect::RectF
};
use pathfinder_view::{Interactive, Context, ElementState, KeyEvent, KeyCode};
use vector::{PathBuilder, PathStyle, Surface, FillRule, Paint};
use unicode_segmentation::UnicodeSegmentation;
use unicode_categories::UnicodeCategories;
use std::borrow::Cow;

pub struct App {
    target: Target,
    document: Document,
    design: Design,

    cache: Cache,
    pages: Vec<Option<Page>>,
    cursor: Option<Cursor>,
}
impl App {
    pub fn from_state(state: State, site: SiteId) -> Self {
        let storage = state.storage.into_owned();
        let target = state.target.into_owned();
        let design = state.design.into_owned();
        let root = state.root;

        let document = Document::from_storage(storage, root, site);
        let cache = Cache::new();

        let mut app = App {
            cache,
            document,
            cursor: None,
            pages: vec![],
            target,
            design
        };
        app.layout();

        app
    }

    pub fn store(&self) {
        let state = State {
            target: Cow::Borrowed(&self.target),
            design: Cow::Borrowed(&self.design),
            storage: Cow::Borrowed(self.document.storage()),
            root: self.document.root()
        };
        state.store(std::fs::File::create("document.graf").unwrap()).unwrap();
    }
    fn layout(&mut self) {
        self.cache.layout(self.document.storage(), &self.design, &self.target, self.document.root());
        let num_pages = self.cache.columns.as_ref().unwrap().len();
        info!("{} pages", num_pages);

        self.pages = std::iter::from_fn(|| Some(None)).take(num_pages).collect();
    }
    fn render_page(&mut self, page_nr: usize) {
        let page = self.cache.render_page(&self.document, &self.target, &self.design, page_nr);
        self.pages[page_nr] = Some(page);
    }
    fn page_position(&self, page_nr: usize, tag: Tag) -> Option<RectF> {
        let page_nr = page_nr as u32;
        if let Some(&(p, r)) = self.cache.positions.get(&tag) {
            if p == page_nr {
                Some(r)
            } else {
                None
            }
        } else if let Some(rendered) = self.cache.word_positions.get(&tag) {
            match *rendered {
                RenderedWord::Full((p, r)) if p == page_nr => Some(r),
                RenderedWord::Before((p, r1), _) if p == page_nr => Some(r1),
                RenderedWord::After((p, r2), _) if p == page_nr => Some(r2),
                RenderedWord::Both((p, r1), _, _) if p == page_nr => Some(r1),
                RenderedWord::Both(_, (p, r2), _) if p == page_nr => Some(r2),
                _ => None
            }
        } else {
            None
        }
    }
    fn get_position(&self, tag: Tag) -> Option<(usize, RectF)> {
        let tag = match tag {
            Tag::Item(_, _) => match self.document.get_item(tag)? {
                Item::Sequence(id) => Tag::End(id),
                Item::Word(_) => {
                    let rendered = self.cache.word_positions.get(&tag)?;
                    let (n, rect) = match *rendered {
                        RenderedWord::Full(r) => r,
                        RenderedWord::Before(r1, _) => r1,
                        RenderedWord::After(r2, _) => r2,
                        RenderedWord::Both(r1, _r2, _) => r1,
                    };
                    return Some((n as usize, rect));
                }
                _ => tag
            }
            _ => tag
        };
        let &(n, p) = self.cache.positions.get(&tag)?;
        debug!("{:?} at {:?} on page {}", tag, p, n);
        Some((n as usize, p))
    }
    fn set_cursor_to(&mut self, ctx: &mut Context, tag: Tag, pos: ItemPos) {
        debug!("set_cursor_to({:?}, {:?}", tag, pos);
        let weave = self.document.get_weave(tag.seq());
        match tag {
            Tag::Start(_) | Tag::End(_) => {
                if let Some((page, rect)) = self.get_position(tag) {
                    self.cursor = Some(Cursor {
                        tag,
                        page,
                        pos,
                        page_pos: rect.lower_left()
                    });
                }
            },
            Tag::Item(_, id) => {
                let item = weave.get_item(id).unwrap();
                match (pos, item) {
                    (ItemPos::After, _) => {
                        if let Some((page, rect)) = self.get_position(tag) {
                            let type_key = weave.typ();
                            let typ = self.design.get_type_or_default(type_key);
                            self.cursor = Some(Cursor {
                                tag,
                                page,
                                pos,
                                page_pos: rect.lower_right() + Vector2F::new(0.5 * typ.word_space.length.value, 0.0),
                            });
                        } else {
                            self.cursor = None;
                        }
                    }
                    (ItemPos::Within(text_pos), Item::Word(_)) => {
                        self.cursor = self.cache.get_position(self.document.storage(), &self.design, tag, text_pos)
                        .map(|(page, page_pos)| Cursor {
                            tag,
                            page,
                            pos,
                            page_pos,
                        });
                    }
                    _ => {}
                }
            }
        }
        debug!("cursor: {:?}", self.cursor);

        if let Some(cursor) = self.cursor {
            ctx.goto_page(cursor.page);
            assert!(cursor.page_pos.x().is_finite());
            assert!(cursor.page_pos.y().is_finite());
        }
    }
    fn text_op(&mut self, op: TextOp) -> Option<(Tag, ItemPos)> {
        let cursor = self.cursor?;

        match cursor.pos {
            ItemPos::Within(n) => {
                match self.document.get_item(cursor.tag)? {
                    Item::Word(word_key) => {
                        let text = &self.document.get_word(word_key).text;

                        match op {
                            TextOp::DeletePrevGrapheme if n > 0 => {
                                let new_pos = text[.. n].grapheme_indices(true).rev().next()
                                    .map(|(n, _)| n).unwrap_or(0);
                                let new_text = format!("{}{}", &text[.. new_pos], &text[n ..]);
                                if new_text.len() == 0 {
                                    let prev_tag = self.document.get_previous_tag(cursor.tag)?;
                                    self.document.remove(cursor.tag);
                                    return Some((prev_tag, ItemPos::After));
                                }
                                let new_item = Item::Word(self.document.create_word(&new_text));
                                let tag = self.document.replace(cursor.tag, new_item);
                                Some((tag, ItemPos::Within(new_pos)))
                            }
                            TextOp::DeleteNextGrapheme if n < text.len() => {
                                let new_pos = text[n ..].grapheme_indices(true).nth(1)
                                    .map(|(m, _)| n + m).unwrap_or(text.len());
                                let new_text = format!("{}{}", &text[.. n], &text[new_pos ..]);
                                if new_text.len() == 0 {
                                    let prev_tag = self.document.get_previous_tag(cursor.tag)?;
                                    self.document.remove(cursor.tag);
                                    return Some((prev_tag, ItemPos::After));
                                }
                                let new_item = Item::Word(self.document.create_word(&new_text));
                                let tag = self.document.replace(cursor.tag, new_item);

                                Some((tag, ItemPos::Within(n)))
                            }
                            TextOp::Insert(c) => {
                                let new_text = format!("{}{}{}", &text[.. n], c, &text[n ..]);
                                let new_item = Item::Word(self.document.create_word(&new_text));
                                let tag = self.document.replace(cursor.tag, new_item);

                                Some((tag, ItemPos::Within(n + c.len_utf8())))
                            }

                            // split, but only when within a word
                            TextOp::Split if n > 0 && n < text.len() => {
                                let left_text = text[.. n].to_owned();
                                let right_text = text[n ..].to_owned();
                                let left_item = Item::Word(self.document.create_word(&left_text));
                                let right_item = Item::Word(self.document.create_word(&right_text));
                                let left_tag = self.document.replace(cursor.tag, left_item);
                                self.document.insert(left_tag, right_item);
                                Some((left_tag, ItemPos::After))
                            }

                            // place cursor before the word
                            TextOp::Split if n == 0 => {
                                let prev_tag = self.document.get_previous_tag(cursor.tag)?;
                                Some((prev_tag, ItemPos::After))
                            }

                            // place cursor behind the word
                            TextOp::Split if n == text.len() => {
                                Some((cursor.tag, ItemPos::After))
                            }

                            TextOp::DeletePrevGrapheme if n == 0 => {
                                // join with previous item … if possible
                                let prev_tag = self.document.get_previous_tag(cursor.tag)?;
                                match self.document.get_item(prev_tag)? {
                                    Item::Word(prev_word_key) => {
                                        let prev_text = &self.document.get_word(prev_word_key).text;
                                        let new_pos = prev_text.len();
                                        let new_text = format!("{}{}", prev_text, text);
                                        let new_item = Item::Word(self.document.create_word(&new_text));
                                        let tag = self.document.replace(prev_tag, new_item);
                                        self.document.remove(cursor.tag);

                                        Some((tag, ItemPos::Within(new_pos)))
                                    }
                                    _ => None
                                }
                            }
                            _ => None
                        }
                    }
                    _ => None
                }
            }
            ItemPos::After => {
                let valid_insert = cursor.tag.item().is_some();
                match op {
                    TextOp::DeletePrevItem => {
                        let prev_tag = self.document.get_previous_tag_bounded(cursor.tag)?;
                        self.document.remove(cursor.tag);
                        Some((prev_tag, ItemPos::After))
                    }
                    TextOp::DeleteNextItem => {
                        let next_tag = self.document.get_next_tag_bounded(cursor.tag)?;
                        self.document.remove(next_tag);
                        Some((cursor.tag, ItemPos::After))
                    }
                    TextOp::Insert(c) if valid_insert => {
                        let new_text = format!("{}", c);
                        let new_item = Item::Word(self.document.create_word(&new_text));
                        let tag = self.document.insert(cursor.tag, new_item);

                        Some((tag, ItemPos::Within(new_text.len())))
                    }
                    // place cursor at the end of the previous word
                    TextOp::DeletePrevGrapheme => {
                        match self.document.get_item(cursor.tag)? {
                            Item::Word(id) => {
                                let text = &self.document.get_word(id).text;
                                Some((cursor.tag, ItemPos::Within(text.len())))
                            }
                            _ => None
                        }
                    }
                    TextOp::NewSequence if valid_insert => {
                        let typ = self.document.get_weave(cursor.tag.seq()).typ();
                        let id = self.document.crate_seq(typ);
                        let item = Item::Sequence(id);
                        self.document.insert(cursor.tag, item);

                        // put cursor at the start of the created sequence
                        Some((Tag::Start(id), ItemPos::After))
                    }
                    _ => None
                }
            }
        }
    }
    fn cursor_op(&mut self, op: CursorOp) -> Option<(Tag, ItemPos)> {
        let cursor = self.cursor?;
        debug!("cursor: {:?}", cursor);
        match (cursor.pos, self.document.get_item(cursor.tag)) {
            (ItemPos::Within(n), Some(Item::Word(word_key))) => {
                let text = &self.document.get_word(word_key).text;
                match op {
                    CursorOp::GraphemeRight if n < text.len() => {
                        // BIDI
                        let pos = text[n ..].grapheme_indices(true).nth(1)
                            .map(|(m, _)| n + m).unwrap_or(text.len());
                        Some((cursor.tag, ItemPos::Within(pos)))
                    }
                    CursorOp::GraphemeLeft if n > 0 => {
                        // BIDI
                        let pos = text[.. n].grapheme_indices(true).rev().next()
                            .map(|(n, _)| n).unwrap_or(0);
                        Some((cursor.tag, ItemPos::Within(pos)))
                    }
                    CursorOp::GraphemeLeft | CursorOp::ItemLeft => {
                        let prev_tag = self.document.get_previous_tag(cursor.tag)?;
                        debug!("prev of {:?}: {:?}", cursor.tag, prev_tag);
                        Some((prev_tag, ItemPos::After))
                    }
                    CursorOp::GraphemeRight | CursorOp::ItemRight => {
                        Some((cursor.tag, ItemPos::After))
                    }
                }
            }
            (ItemPos::After, _) => {
                match op {
                    CursorOp::GraphemeLeft => {
                        match self.document.get_item(cursor.tag) {
                            Some(Item::Word(id)) => {
                                let text = &self.document.get_word(id).text;
                                Some((cursor.tag, ItemPos::Within(text.len())))
                            },
                            _ => {
                                let prev_tag = self.document.get_previous_tag(cursor.tag)?;
                                debug!("get_previous_tag({:?}) = {:?}", cursor.tag, prev_tag);
                                Some((prev_tag, ItemPos::After))
                            }
                        }
                    },
                    CursorOp::GraphemeRight => {
                        let next_tag = self.document.get_next_tag(cursor.tag)?;
                        debug!("get_next_tag({:?}) = {:?}", cursor.tag, next_tag);
                        match self.document.get_item(next_tag) {
                            Some(Item::Word(_)) => Some((next_tag, ItemPos::Within(0))),
                            _ => Some((next_tag, ItemPos::After))
                        }
                    },
                    CursorOp::ItemLeft => {
                        let left_tag = self.document.get_previous_tag(cursor.tag)?;
                        Some((left_tag, ItemPos::After))
                    }
                    CursorOp::ItemRight => {
                        let right_tag = self.document.get_next_tag(cursor.tag)?;
                        Some((right_tag, ItemPos::After))
                    }
                }
            }
            _ => None
        }
    }

    pub fn op(&mut self, op: DocumentOp) {
        self.document.exec_op(op);
        self.layout();
    }

    pub fn pending<'s>(&'s mut self) -> impl Iterator<Item=DocumentOp> + 's {
        self.document.drain_pending()
    }
}

enum TextOp {
    Insert(char),
    Split,
    DeletePrevGrapheme,
    DeleteNextGrapheme,
    DeletePrevItem,
    DeleteNextItem,
    NewSequence,
}
enum CursorOp {
    GraphemeLeft,
    GraphemeRight,
    ItemRight,
    ItemLeft,
}


#[derive(PartialEq, Copy, Clone, Debug)]
enum ItemPos {
    After,
    Within(usize)
}

#[derive(PartialEq, Copy, Clone, Debug)]
struct Cursor {
    tag: Tag,   // which item
    page: usize,
    pos: ItemPos, // between this and the following
    page_pos: Vector2F,
}

impl Interactive for App {
    fn title(&self) -> String {
        "γραφείο".into()
    }
    fn num_pages(&self) -> usize {
        self.pages.len()
    }
    fn scene(&mut self, page_nr: usize) -> Scene {
        if self.pages.get(page_nr).expect("page out of bounds").is_none() {
            self.render_page(page_nr);
        }
        let mut scene = self.pages[page_nr].as_ref().unwrap().scene().clone();
        if let Some(ref cursor) = self.cursor {
            let weave = self.document.get_weave(cursor.tag.seq());
            let type_design = self.design.get_type_or_default(weave.typ());
            let style = scene.build_style(PathStyle {
                fill: None,
                stroke: Some((Paint::Solid((0,0,200,255)), 0.1 * type_design.font.size.value)),
                fill_rule: FillRule::NonZero
            });
            let mut pb = PathBuilder::new();
            pb.move_to(cursor.page_pos);
            pb.line_to(cursor.page_pos - Vector2F::new(0.0, type_design.font.size.value));
            
            scene.draw_path(pb.into_outline(), &style, None);

            let mark_style = scene.build_style(PathStyle {
                fill: None,
                stroke: Some((Paint::Solid((100,0,200,255)), 0.05 * type_design.font.size.value)),
                fill_rule: FillRule::NonZero
            });
            let underline_style = scene.build_style(PathStyle {
                fill: None,
                stroke: Some((Paint::Solid((0,200,0,255)), 0.2)),
                fill_rule: FillRule::NonZero
            });

            let mark_seq_end = |scene: &mut Scene, p: Vector2F, w: f32| {
                let dx = Vector2F::new(w, 0.0);
                let q = p - Vector2F::new(0.0, type_design.line_height.value);
                let mut pb = PathBuilder::new();
                pb.move_to(p);
                pb.cubic_curve_to(p + dx, q + dx, q);
                scene.draw_path(pb.into_outline(), &mark_style, None);
            };
            let word_space = type_design.word_space.length.value;
            let mark_seq = |scene: &mut Scene, tag: Tag| {
                match tag {
                    Tag::Start(seq) => {
                        if let Some(rect) = self.page_position(page_nr, Tag::End(seq)) {
                            mark_seq_end(scene, rect.lower_left(), 0.5 * word_space);
                        }
                    }
                    Tag::End(seq) => {
                        if let Some(rect) = self.page_position(page_nr, Tag::Start(seq)) {
                            mark_seq_end(scene, rect.lower_right(), -0.5 * word_space);
                        }
                    }
                    _ => {}
                }
            };
            let mark_word = |scene: &mut Scene, tag: Tag| {
                if let Some(rect) = self.page_position(0, tag) {
                    let mut pb = PathBuilder::new();
                    pb.move_to(rect.lower_left());
                    pb.line_to(rect.lower_right());
                    scene.draw_path(pb.into_outline(), &underline_style, None);
                }
            };
            mark_seq(&mut scene, cursor.tag);

            match self.document.get_item(cursor.tag) {
                Some(Item::Word(_)) => {
                    mark_word(&mut scene, cursor.tag);
                }
                Some(Item::Sequence(key)) => {
                    for child in self.document.childen(key) {
                        mark_word(&mut scene, child);
                    }
                    mark_seq(&mut scene, Tag::Start(key));
                    mark_seq(&mut scene, Tag::End(key));
                }
                Some(Item::Object(_)) => {
                    let outline_style = scene.build_style(PathStyle {
                        fill: None,
                        stroke: Some((Paint::Solid((200,0,0,255)), 0.2)),
                        fill_rule: FillRule::NonZero
                    });
        
                    if let Some(rect) = self.page_position(page_nr, cursor.tag) {
                        let mut pb = PathBuilder::new();
                        pb.rect(rect);
                        scene.draw_path(pb.into_outline(), &outline_style, None);
                    }
                }
                _ => {}
            }
        }

        scene
    }
    fn mouse_input(&mut self, ctx: &mut Context, page: usize, pos: Vector2F, state: ElementState) {
        let old_cursor = self.cursor.take();

        if let Some((tag, word_pos)) = self.pages[page].as_ref().unwrap().find(pos) {
            let offset = pos.x() - word_pos.x();

            if let Some((word_offset, n)) = self.cache.find(self.document.storage(), &self.design, offset, tag) {
                self.cursor = Some(Cursor {
                    tag,
                    page,
                    page_pos: word_offset + word_pos,
                    pos: ItemPos::Within(n)
                });
            } else {
                self.set_cursor_to(ctx, tag, ItemPos::After);
            }
        }

        if self.cursor != old_cursor {
            ctx.update_scene();
        }
    }

    fn keyboard_input(&mut self, ctx: &mut Context, event: &mut KeyEvent) {
        if event.state == ElementState::Released {
            return;
        }

        let (update, s) = match (event.keycode, event.modifiers.shift) {
            (KeyCode::Right, false) => (false, self.cursor_op(CursorOp::GraphemeRight)),
            (KeyCode::Right, true) => (false, self.cursor_op(CursorOp::ItemRight)),
            (KeyCode::Left, false) => (false, self.cursor_op(CursorOp::GraphemeLeft)),
            (KeyCode::Left, true) => (false, self.cursor_op(CursorOp::ItemLeft)),
            (KeyCode::Back, false) => (true, self.text_op(TextOp::DeletePrevGrapheme)),
            (KeyCode::Back, true) => (true, self.text_op(TextOp::DeletePrevItem)),
            (KeyCode::Delete, false) => (true, self.text_op(TextOp::DeleteNextGrapheme)),
            (KeyCode::Delete, true) => (true, self.text_op(TextOp::DeleteNextItem)),
            (KeyCode::Return, false) => (true, self.text_op(TextOp::NewSequence)),
            (KeyCode::PageUp, false) => return ctx.prev_page(),
            (KeyCode::PageDown, false) => return ctx.next_page(),
            _ => return
        };
        if update & s.is_some() {
            self.layout();
        }
        if let Some((tag, pos)) = s {
            self.set_cursor_to(ctx, tag, pos);
            ctx.update_scene();
        }
    }

    fn char_input(&mut self, ctx: &mut Context, c: char) {
        let s = match c {
            // backspace
            ' ' => self.text_op(TextOp::Split),
            c if c.is_letter() => self.text_op(TextOp::Insert(c)),
            _ => None
        };
        if let Some((tag, pos)) = s {
            self.layout();
            self.set_cursor_to(ctx, tag, pos);
            ctx.update_scene();
        }
    }
    fn exit(&mut self, _ctx: &mut Context) {
        //self.store()
    }
}
