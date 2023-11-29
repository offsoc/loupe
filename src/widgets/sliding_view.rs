// Copyright (c) 2023 Sophie Herold
// Copyright (c) 2023 Alice Mikhaylenko
// Copyright (c) 2023 Christopher Davis
// Copyright (c) 2023 Lubosz Sarnecki
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! A sliding view widget for images
//!
//! This widget it similar to `AdwCarousel`.

use crate::deps::*;
use crate::widgets::LpImagePage;

use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::Properties;
use indexmap::IndexMap;
use once_cell::sync::Lazy;

use std::cell::{Cell, OnceCell, RefCell};

static SCROLL_DAMPING_RATIO: f64 = 1.;
static SCROLL_MASS: f64 = 0.5;
static SCROLL_STIFFNESS: f64 = 500.;

/// Space between images in application pixels
///
/// This is combined with the percent component
static PAGE_SPACING_FIXED: f32 = 25.;

/// Space between images as factor of width
const PAGE_SPACING_PERCENT: f32 = 0.02;

mod imp {
    use super::*;
    use glib::subclass::Signal;

    #[derive(Default, Properties)]
    #[properties(wrapper_type = super::LpSlidingView)]
    pub struct LpSlidingView {
        /// Pages that can be slided through
        pub(super) pages: RefCell<Vec<LpImagePage>>,
        /// Page that is currently shown or the animation is animating towards
        #[property(get)]
        pub(super) current_page: RefCell<Option<LpImagePage>>,
        /// Animatable position, 0.0 first image, 1.0 second image etc
        pub(super) position: Cell<f64>,
        /// Move position to not break animations when pages are removed/added
        pub(super) position_shift: Cell<f64>,
        /// The animation used to animate image changes
        pub(super) scroll_animation: OnceCell<adw::SpringAnimation>,
        /// Implements swiping
        pub(super) swipe_tracker: OnceCell<adw::SwipeTracker>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LpSlidingView {
        const NAME: &'static str = "LpSlidingView";
        type Type = super::LpSlidingView;
        type ParentType = gtk::Widget;
        type Interfaces = (adw::Swipeable,);
    }

    impl ObjectImpl for LpSlidingView {
        fn properties() -> &'static [glib::ParamSpec] {
            Self::derived_properties()
        }

        fn property(&self, id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            self.derived_property(id, pspec)
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> =
                Lazy::new(|| vec![Signal::builder("target-page-reached").build()]);
            SIGNALS.as_ref()
        }

        fn constructed(&self) {
            let obj = self.obj();
            self.parent_constructed();

            self.obj().set_overflow(gtk::Overflow::Hidden);

            let swipe_tracker = adw::SwipeTracker::builder()
                .swipeable(&*self.obj())
                .reversed(self.is_rtl())
                .lower_overshoot(true)
                .upper_overshoot(true)
                .build();

            swipe_tracker.connect_begin_swipe(
                glib::clone!(@weak obj => move |_| obj.scroll_animation().pause()),
            );

            swipe_tracker.connect_update_swipe(glib::clone!(@weak obj => move |_, position| {
                obj.set_position(position);
            }));

            swipe_tracker.connect_end_swipe(glib::clone!(@weak obj => move |_, velocity, to| {
                if let Some(page) = obj.page_at(to) {
                    obj.scroll_to_velocity(&page, velocity);
                }
            }));

            self.swipe_tracker.set(swipe_tracker).unwrap();

            // Avoid propagating scroll events to AdwFlap if at beginning or end
            let scroll_controller =
                gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::HORIZONTAL);

            scroll_controller.connect_scroll(
                glib::clone!(@weak obj => @default-return glib::Propagation::Proceed, move |_, x, _| {
                    let direction_sign = if obj.imp().is_rtl() { -1. } else { 1. };

                    if x * direction_sign > 0. {
                        // check end
                        if let Some(max) = obj.imp().snap_points().last() {
                            if obj.position() >= *max {
                                glib::Propagation::Stop
                            } else {
                                glib::Propagation::Proceed
                            }
                        } else {
                            glib::Propagation::Stop
                        }
                    } else {
                        //check beginning
                        if let Some(min) = obj.imp().snap_points().first() {
                            if obj.position() <= *min {
                                glib::Propagation::Stop
                            } else {
                                glib::Propagation::Proceed
                            }
                        } else {
                            glib::Propagation::Stop
                        }
                    }
                }),
            );

            obj.add_controller(scroll_controller);
        }

        fn dispose(&self) {
            let obj = self.obj();
            while let Some(child) = obj.first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for LpSlidingView {
        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let scroll_position = self.position.get() as f32;
            let position_shift = self.position_shift.get() as f32;

            for (page_index, page) in self.pages.borrow().iter().enumerate() {
                let page_position = page_index as f32;

                // reverse page order for RTL languages
                let direction_sign = if self.is_rtl() { -1. } else { 1. };

                // This positions the pages within the carousel and shifts them
                // according to the position that should currently be shown.
                let x = direction_sign
                    * (page_position - scroll_position - position_shift)
                    * (width as f32 + self.page_spacing(width));

                // Only show visible images
                if page_position == (scroll_position + position_shift).floor()
                    || page_position == (scroll_position + position_shift).ceil()
                {
                    page.set_child_visible(true);
                    let transform = gsk::Transform::new().translate(&graphene::Point::new(x, 0.));
                    page.allocate(width, height, 0, Some(transform));
                } else {
                    page.set_child_visible(false);
                }

                if !page.scrolled_window().is_mapped() {
                    // SVG needs to know a widget size before rendering something in LpImage
                    page.scrolled_window().allocate(width, height, 0, None);
                }
            }
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let obj = self.obj();

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

        fn direction_changed(&self, _previous_direction: gtk::TextDirection) {
            self.swipe_tracker
                .get()
                .unwrap()
                .set_reversed(self.is_rtl());
        }
    }

    impl SwipeableImpl for LpSlidingView {
        fn progress(&self) -> f64 {
            self.obj().position()
        }

        fn distance(&self) -> f64 {
            let obj = self.obj();
            // TODO: Fix deprecation
            #[allow(deprecated)]
            let width = obj.allocation().width();

            width as f64 + self.page_spacing(width) as f64
        }

        fn snap_points(&self) -> Vec<f64> {
            let obj = self.obj();

            (0..obj.n_pages())
                .map(|i| i as f64 - obj.position_shift())
                .collect()
        }

        fn cancel_progress(&self) -> f64 {
            let obj = self.obj();
            let snap_points = self.snap_points();

            if let (Some(min), Some(max)) = (snap_points.first(), snap_points.last()) {
                obj.position().round().clamp(*min, *max)
            } else {
                0.
            }
        }
    }

    impl LpSlidingView {
        fn page_spacing(&self, width: i32) -> f32 {
            width as f32 * PAGE_SPACING_PERCENT + PAGE_SPACING_FIXED
        }

        fn is_rtl(&self) -> bool {
            let obj = self.obj();
            obj.direction() == gtk::TextDirection::Rtl
        }
    }
}

glib::wrapper! {
    pub struct LpSlidingView(ObjectSubclass<imp::LpSlidingView>)
        @extends gtk::Widget,
        @implements gtk::Buildable, adw::Swipeable;
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
        if pos > self.n_pages() {
            log::error!("Invalid insert position {pos} for {}", page.file().uri());
            return;
        }

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
            log::error!("Page to move not found: {}", page.file().uri());
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
    pub fn pages(&self) -> IndexMap<glib::GString, LpImagePage> {
        self.imp()
            .pages
            .borrow()
            .iter()
            .cloned()
            .map(|x| (x.file().uri(), x))
            .collect()
    }

    pub fn get(&self, file: &gio::File) -> Option<LpImagePage> {
        let uri = file.uri();
        self.imp()
            .pages
            .borrow()
            .iter()
            .find(|x| x.file().uri() == uri)
            .cloned()
    }

    /// Move to specified page with animation
    pub fn scroll_to(&self, page: &LpImagePage) {
        self.scroll_to_velocity(page, 0.);
    }

    pub fn scroll_to_velocity(&self, page: &LpImagePage, initial_velocity: f64) {
        if let Some(index) = self.index_of(page) {
            let animation = self.scroll_animation();

            animation.set_value_from(self.position());
            animation.set_value_to(index as f64 - self.position_shift());
            animation.set_initial_velocity(initial_velocity);
            animation.play();
            self.set_current_page(Some(page));
        } else {
            log::error!("Page not in LpSlidingView {}", page.file().uri());
        }
    }

    /// Animates removal of current page
    ///
    /// Moves to next page if possible, or to previous, or immediately removes
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

    fn page_at(&self, position: f64) -> Option<LpImagePage> {
        let index = (position + self.position_shift()) as usize;
        self.imp().pages.borrow().get(index).cloned()
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

    fn scroll_animation(&self) -> &adw::SpringAnimation {
        self.imp().scroll_animation.get_or_init(|| {
            let target = adw::CallbackAnimationTarget::new(
                glib::clone!(@weak self as obj => move |position| obj.set_position(position)),
            );

            let animation = adw::SpringAnimation::builder()
                .spring_params(&adw::SpringParams::new(
                    SCROLL_DAMPING_RATIO,
                    SCROLL_MASS,
                    SCROLL_STIFFNESS,
                ))
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
