//! Sliding view for images
//!
//! This widget it similar to `AdwCarousel`.
//! Currently, there is no swipe-gesture support since it's not working with
//! `GtkScrolledWindow` anyways.

use crate::deps::*;
use crate::widgets::LpImagePage;

use adw::prelude::*;
use adw::subclass::prelude::*;
use indexmap::IndexMap;
use once_cell::sync::{Lazy, OnceCell};

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};

static SCROLL_DURATION: u32 = 200;

mod imp {
    use super::*;
    use glib::subclass::Signal;

    #[derive(Default)]
    pub struct LpSlidingView {
        /// Pages that can be slided through
        pub(super) pages: RefCell<Vec<LpImagePage>>,
        /// Page that is currently shown or the animation is animating towards
        pub(super) current_page: RefCell<Option<LpImagePage>>,
        /// Animatable position, 0.0 first image, 1.0 second image etc
        pub(super) position: Cell<f64>,
        /// Move position to not break animations when pages are removed/added
        pub(super) position_shift: Cell<f64>,
        /// The animation used to animate image changes
        pub(super) scroll_animation: OnceCell<adw::TimedAnimation>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LpSlidingView {
        const NAME: &'static str = "LpSlidingView";
        type Type = super::LpSlidingView;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for LpSlidingView {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<LpImagePage>("current-page")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.instance();
            match pspec.name() {
                "current-page" => obj.current_page().to_value(),
                name => unimplemented!("property {name}"),
            }
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> =
                Lazy::new(|| vec![Signal::builder("target-page-reached").build()]);
            SIGNALS.as_ref()
        }

        fn constructed(&self) {
            self.instance().set_overflow(gtk::Overflow::Hidden);
        }
    }

    impl WidgetImpl for LpSlidingView {
        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let scroll_position = self.position.get() as f32;
            let position_shift = self.position_shift.get() as f32;

            for (page_index, page) in self.pages.borrow().iter().enumerate() {
                // This positions the pages within the carousel and shifts them
                // according to the position that should currently be shown.
                let x = (page_index as f32 - scroll_position - position_shift) * width as f32;

                let transform = gsk::Transform::new().translate(&graphene::Point::new(x, 0.));
                page.allocate(width, height, 0, Some(&transform));
            }
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let obj = self.instance();

            for page in self.pages.borrow().iter() {
                if Some(page) != obj.current_page().as_ref() {
                    page.measure(orientation, for_size);
                }
            }

            if let Some(current_page) = obj.current_page() {
                current_page.measure(orientation, for_size)
            } else {
                (0, 0, -1, -1)
            }
        }
    }
}

glib::wrapper! {
    pub struct LpSlidingView(ObjectSubclass<imp::LpSlidingView>)
        @extends gtk::Widget,
        @implements gtk::Buildable;
}

impl LpSlidingView {
    /// Add page to the end
    pub fn append(&self, page: &LpImagePage) {
        self.insert(page, self.n_pages());
    }

    /// Add page to the front
    pub fn prepend(&self, page: &LpImagePage) {
        self.insert(page, 0);
    }

    /// Add page to the specified position
    pub fn insert(&self, page: &LpImagePage, pos: usize) {
        let current_index = self.current_index();

        let previous_sibling = self.imp().pages.borrow().get(pos).cloned();
        self.imp().pages.borrow_mut().insert(pos, page.clone());
        page.insert_after(self, previous_sibling.as_ref());

        self.shift_position(current_index);

        if self.n_pages() == 1 {
            self.set_current_page(Some(page));
        }
    }

    /// Reorders the pages by moving the specified page to the new index
    pub fn move_to(&self, page: &LpImagePage, new_index: usize) {
        if let Some(index) = self.index_of(page) {
            let current_index = self.current_index();
            {
                let mut vec = self.imp().pages.borrow_mut();
                let page = vec.remove(index);
                vec.insert(new_index, page);
            }
            self.shift_position(current_index);
        } else {
            log::error!("Page to move not found");
        }
    }

    /// Removes the page
    pub fn remove(&self, page: &LpImagePage) {
        let current_index = self.current_index();

        if let Some(index) = self.index_of(page) {
            self.imp().pages.borrow_mut().remove(index);
            page.unparent();

            self.queue_allocate();

            self.shift_position(current_index);

            if self.is_empty() {
                self.clear();
            }
        } else {
            log::error!("Trying to remove non-existent page.");
        }
    }

    /// Signal that fires when the animation for scrolling to new page ends
    pub fn connect_target_page_reached(&self, f: impl Fn() + 'static) {
        self.connect_local("target-page-reached", false, move |_| {
            f();
            None
        });
    }

    /// Removes all pages
    pub fn clear(&self) {
        self.scroll_animation().pause();

        for page in self.imp().pages.borrow().iter() {
            page.unparent();
        }

        self.imp().pages.borrow_mut().clear();
        self.set_position(0.);
        self.imp().position_shift.set(0.);

        self.set_current_page(None);
    }

    /// Gives all pages with hash access via their path
    pub fn pages(&self) -> IndexMap<PathBuf, LpImagePage> {
        self.imp()
            .pages
            .borrow()
            .iter()
            .cloned()
            .map(|x| (x.path(), x))
            .collect()
    }

    pub fn get(&self, path: &Path) -> Option<LpImagePage> {
        self.imp()
            .pages
            .borrow()
            .iter()
            .find(|x| x.path() == path)
            .cloned()
    }

    /// Move to specified page with animation
    pub fn scroll_to(&self, page: &LpImagePage) {
        if self.current_page().as_ref() != Some(page) {
            let index = self.index_of(page).unwrap();

            let animation = self.scroll_animation();

            animation.set_value_from(self.position());
            animation.set_value_to(index as f64 - self.position_shift());
            animation.play();
            self.set_current_page(Some(page));
        }
    }

    /// Animates removal of current page
    ///
    /// Moves to next page if possible, or to previous, or emmediately removes
    /// the current page if last remaining.
    pub fn scroll_to_neighbor(&self) {
        if let Some(next_page) = self.next_page() {
            self.scroll_to(&next_page);
        } else if let Some(prev_page) = self.prev_page() {
            self.scroll_to(&prev_page);
        } else if let Some(current_page) = self.current_page() {
            self.remove(&current_page);
        }
    }

    fn next_page(&self) -> Option<LpImagePage> {
        let current_index = self.current_index()?;
        let next_index = current_index.checked_add(1)?;
        self.imp().pages.borrow().get(next_index).cloned()
    }

    fn prev_page(&self) -> Option<LpImagePage> {
        let current_index = self.current_index()?;
        let prev_index = current_index.checked_sub(1)?;
        self.imp().pages.borrow().get(prev_index).cloned()
    }

    fn n_pages(&self) -> usize {
        self.imp().pages.borrow().len()
    }

    fn is_empty(&self) -> bool {
        self.imp().pages.borrow().is_empty()
    }

    fn index_of(&self, page: &LpImagePage) -> Option<usize> {
        self.imp().pages.borrow().iter().position(|x| x == page)
    }

    fn position(&self) -> f64 {
        self.imp().position.get()
    }

    fn position_shift(&self) -> f64 {
        self.imp().position_shift.get()
    }

    /// Sets position shift to correct page insert or removal
    ///
    /// Takes the old position of the current page and calculates and sets the shift
    /// to keep the current page optically at the same position.
    fn shift_position(&self, old_index: Option<usize>) {
        if let (Some(old_index), Some(new_index)) = (old_index, self.current_index()) {
            let shift = new_index as f64 - old_index as f64;
            self.imp().position_shift.set(self.position_shift() + shift);
        }
    }

    fn set_position(&self, position: f64) {
        self.imp().position.set(position);
        self.queue_allocate();
    }

    pub fn current_page(&self) -> Option<LpImagePage> {
        self.imp().current_page.borrow().clone()
    }

    fn set_current_page(&self, page: Option<&LpImagePage>) {
        self.imp().current_page.replace(page.cloned());
        self.notify("current-page");
    }

    fn current_index(&self) -> Option<usize> {
        self.imp()
            .current_page
            .borrow()
            .as_ref()
            .and_then(|x| self.index_of(x))
    }

    fn scroll_animation(&self) -> &adw::TimedAnimation {
        self.imp().scroll_animation.get_or_init(|| {
            let target = adw::CallbackAnimationTarget::new(
                glib::clone!(@weak self as obj => move |position| obj.set_position(position)),
            );

            let animation = adw::TimedAnimation::builder()
                .duration(SCROLL_DURATION)
                .widget(self)
                .target(&target)
                .build();

            animation.connect_done(glib::clone!(@weak self as obj => move |_| {
                obj.emit_by_name("target-page-reached", &[])
            }));

            animation
        })
    }
}