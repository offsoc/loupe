// Copyright (c) 2023 Sophie Herold
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

//! The print layout widget that also coordinates the print process

use crate::decoder;
use crate::deps::*;
use crate::util::gettext::*;
use crate::widgets::{LpImage, LpPrintPreview};

use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::Properties;
use gtk::CompositeTemplate;
use once_cell::sync::OnceCell;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Default, Clone, Copy)]
struct PageAlignment {
    horizontal: HAlignment,
    vertical: VAlignment,
}

#[derive(Default, Clone, Copy)]
enum HAlignment {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Default, Clone, Copy)]
enum VAlignment {
    Top,
    #[default]
    Middle,
    Bottom,
}

/// Scope guard for non user ui changes
///
/// Creates a context in which other signals know that changes are not user input
#[derive(Default, Clone, Debug)]
struct UiUpdates {
    disabled: Rc<Cell<bool>>,
}

impl UiUpdates {
    pub fn disable(&self) -> Self {
        self.disabled.set(true);
        self.clone()
    }
}

impl Drop for UiUpdates {
    fn drop(&mut self) {
        self.disabled.set(false);
    }
}

impl From<&str> for PageAlignment {
    fn from(s: &str) -> Self {
        let alignment = match s {
            "top" => (HAlignment::Center, VAlignment::Top),
            "center" => (HAlignment::Center, VAlignment::Middle),
            "bottom" => (HAlignment::Center, VAlignment::Bottom),
            "left" => (HAlignment::Left, VAlignment::Middle),
            "right" => (HAlignment::Right, VAlignment::Middle),
            pos => {
                log::error!("Unkown alignment '{pos}'");
                (HAlignment::default(), VAlignment::default())
            }
        };

        Self::from(alignment)
    }
}

impl From<(HAlignment, VAlignment)> for PageAlignment {
    fn from((horizontal, vertical): (HAlignment, VAlignment)) -> Self {
        Self {
            horizontal,
            vertical,
        }
    }
}

impl std::fmt::Display for PageAlignment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let s = match (self.horizontal, self.vertical) {
            (HAlignment::Center, VAlignment::Top) => gettext("Top"),
            (HAlignment::Center, VAlignment::Middle) => gettext("Center"),
            (HAlignment::Center, VAlignment::Bottom) => gettext("Bottom"),
            (HAlignment::Left, VAlignment::Middle) => gettext("Left"),
            (HAlignment::Right, VAlignment::Middle) => gettext("Right"),
            _ => String::from("Unsupported (Error)"),
        };

        f.write_str(&s)
    }
}

#[derive(Default, Clone, Copy)]
enum Unit {
    Centimeter,
    Inch,
    #[default]
    Pixel,
    Percent,
}

impl<T: AsRef<str>> From<T> for Unit {
    fn from(s: T) -> Self {
        match s.as_ref() {
            "cm" => Self::Centimeter,
            "in" => Self::Inch,
            "px" => Self::Pixel,
            "%" => Self::Percent,
            unit => {
                log::error!("Unknown unit '{unit}'");
                Self::default()
            }
        }
    }
}

impl std::fmt::Display for Unit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let s = match self {
            // Translators: Shorthand for centimeter
            Self::Centimeter => gettext("cm"),
            // Translators: Shorthand for inch
            Self::Inch => gettext("in"),
            // Translators: Shorthand for pixel
            Self::Pixel => gettext("px"),
            Self::Percent => String::from("%"),
        };

        f.write_str(&s)
    }
}

impl Unit {
    fn factor(&self, dpi: f64, total_size: i32) -> f64 {
        match self {
            Self::Centimeter => 2.54 / dpi,
            Self::Inch => 1. / dpi,
            Self::Pixel => 1.,
            Self::Percent => 100. / total_size as f64,
        }
    }

    fn digits(&self) -> u32 {
        match self {
            Self::Centimeter => 2,
            Self::Inch => 2,
            Self::Pixel => 0,
            Self::Percent => 2,
        }
    }

    fn ceil(&self, num: f64) -> f64 {
        let f = 10_f64.powi(self.digits() as i32);
        (num * f).ceil() / f
    }

    fn floor(&self, num: f64) -> f64 {
        let f = 10_f64.powi(self.digits() as i32);
        (num * f).floor() / f
    }

    fn round(&self, num: f64) -> f64 {
        let f = 10_f64.powi(self.digits() as i32);
        (num * f).round() / f
    }

    fn expression() -> gtk::ClosureExpression {
        gtk::ClosureExpression::new::<glib::GString>(
            [] as [gtk::Expression; 0],
            glib::closure!(|s: gtk::StringObject| Self::from(s.string().as_str()).to_string()),
        )
    }
}

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate, Properties)]
    #[template(file = "../../data/gtk/print.ui")]
    #[properties(wrapper_type = super::LpPrint)]
    pub struct LpPrint {
        #[template_child]
        pub(super) title: TemplateChild<adw::WindowTitle>,
        #[template_child]
        pub(super) preview: TemplateChild<LpPrintPreview>,

        // ListBox entries
        #[template_child]
        pub(super) alignment: TemplateChild<adw::ComboRow>,

        #[template_child]
        pub(super) margin_unit: TemplateChild<gtk::DropDown>,
        #[property(get, set)]
        pub(super) margin_unit_factor: Cell<f64>,
        #[template_child]
        pub(super) margin_horizontal: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub(super) margin_vertical: TemplateChild<adw::SpinRow>,

        #[template_child]
        pub(super) size_unit: TemplateChild<gtk::DropDown>,
        #[property(get, set)]
        pub(super) width_unit_factor: Cell<f64>,
        #[property(get, set)]
        pub(super) height_unit_factor: Cell<f64>,
        #[template_child]
        pub(super) fill_space: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub(super) width: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub(super) height: TemplateChild<adw::SpinRow>,

        #[property(get, set, construct_only)]
        pub(super) image: OnceCell<LpImage>,
        #[property(get, set, construct_only)]
        pub(super) parent_window: OnceCell<gtk::Window>,
        #[property(get, set, construct_only)]
        pub(super) print_settings: OnceCell<gtk::PrintSettings>,
        #[property(get)]
        pub(super) print_operation: gtk::PrintOperation,

        #[property(get, set=Self::set_orientation)]
        pub(super) orientation: RefCell<String>,

        pub(super) ui_updates: UiUpdates,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LpPrint {
        const NAME: &'static str = "LpPrint";
        type Type = super::LpPrint;
        type ParentType = adw::Window;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            Self::Type::bind_template_callbacks(klass);

            klass.install_action_async("print.print", None, |print, _, _| async move {
                print.print();
            });

            klass.install_action_async("print.back", None, |print, _, _| async move {
                print.back().await;
            });

            klass.install_property_action("print.orientation", "orientation");
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LpPrint {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            obj.set_width_unit_factor(1.);
            obj.set_height_unit_factor(1.);
            obj.set_margin_unit_factor(1.);

            obj.set_transient_for(Some(&obj.parent_window()));
            obj.set_modal(true);

            self.alignment
                .connect_selected_notify(glib::clone!(@weak obj => move |_| obj.draw_preview()));

            self.alignment
                .set_expression(Some(gtk::ClosureExpression::new::<glib::GString>(
                    [] as [gtk::Expression; 0],
                    glib::closure!(
                        |s: gtk::StringObject| PageAlignment::from(s.string().as_str()).to_string()
                    ),
                )));

            // Margin signals
            self.margin_unit.connect_selected_notify(
                glib::clone!(@weak obj => move |_| obj.on_margin_unit_changed()),
            );
            self.margin_unit.set_expression(Some(Unit::expression()));
            self.margin_horizontal.connect_value_notify(
                glib::clone!(@weak obj => move |_| obj.on_margin_horizontal_changed()),
            );
            self.margin_vertical.connect_value_notify(
                glib::clone!(@weak obj => move |_| obj.on_margin_vertical_changed()),
            );

            // Size signals
            self.size_unit.connect_selected_notify(
                glib::clone!(@weak obj => move |_| obj.on_size_unit_changed()),
            );
            self.size_unit.set_expression(Some(Unit::expression()));
            self.fill_space.connect_active_notify(
                glib::clone!(@weak obj => move |_| obj.on_fill_space_changed()),
            );
            self.width
                .connect_value_notify(glib::clone!(@weak obj => move |_| obj.on_width_changed()));
            self.height
                .connect_value_notify(glib::clone!(@weak obj => move |_| obj.on_height_changed()));

            self.print_operation
                .set_print_settings(Some(&obj.print_settings()));
            self.print_operation.set_allow_async(true);
            self.print_operation.set_n_pages(1);
            self.print_operation.set_current_page(0);

            if let Some(uri) = obj.image().file().map(|x| x.uri()) {
                obj.print_settings()
                    .set(gtk::PRINT_SETTINGS_OUTPUT_URI, Some(&format!("{uri}.pdf")));
            }

            self.print_operation.connect_begin_print(move |op, _ctx| {
                op.set_n_pages(1);
            });

            self.print_operation.connect_draw_page(glib::clone!(@weak obj =>
                move |operation, _context, _page_nr| {
                    let imp = obj.imp();

                    let basename = obj
                        .image()
                        .file()
                        .and_then(|f| f.basename())
                        .map(|x| x.display().to_string())
                        .unwrap_or_default();
                    imp.title.set_title(&gettext_f(
                        // Translators: {} is a placeholder for the filename
                        "Print “{}”",
                        &[&basename],
                    ));
                    if let Some(printer) = operation.print_settings().and_then(|x| x.printer()) {
                        imp.title.set_subtitle(&printer);
                    }

                    imp.margin_horizontal.adjustment().set_step_increment(0.5);
                    imp.margin_horizontal.adjustment().set_page_increment(2.5);
                    imp.margin_vertical.adjustment().set_step_increment(0.5);
                    imp.margin_vertical.adjustment().set_page_increment(2.5);

                    imp.width.adjustment().set_step_increment(1.);
                    imp.width.adjustment().set_page_increment(5.);
                    imp.height.adjustment().set_step_increment(1.);
                    imp.height.adjustment().set_page_increment(5.);

                    let _ui_updates_disabled = obj.disable_ui_updates();
                    obj.set_ranges();
                    obj.on_margin_unit_changed();
                    obj.on_size_unit_changed();

                    let size_unit = obj.size_unit();
                    imp.width.set_value(
                        size_unit
                            .round(obj.image().image_size().0 as f64 * obj.width_unit_factor()),
                    );

                    imp.print_operation.cancel();

                    let orientation = match obj.page_setup().orientation() {
                        gtk::PageOrientation::Portrait => "portrait",
                        gtk::PageOrientation::Landscape => "landscape",
                        _ => "other",
                    };
                    obj.set_orientation(orientation);

                    obj.set_visible(true);
                }
            ));
        }

        fn properties() -> &'static [glib::ParamSpec] {
            Self::derived_properties()
        }
        fn set_property(&self, id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            self.derived_set_property(id, value, pspec)
        }
        fn property(&self, id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            self.derived_property(id, pspec)
        }
    }

    impl WidgetImpl for LpPrint {}
    impl WindowImpl for LpPrint {}
    impl AdwWindowImpl for LpPrint {}

    impl LpPrint {
        pub fn set_orientation(&self, orientation: String) {
            let obj = self.obj();
            let _ui_updates_disabled = obj.disable_ui_updates();

            self.orientation.replace(orientation);

            let orientation = if obj.orientation() == "landscape" {
                gtk::PageOrientation::Landscape
            } else {
                gtk::PageOrientation::Portrait
            };
            obj.page_setup().set_orientation(orientation);
            obj.set_ranges();
            obj.draw_preview();
        }
    }
}
glib::wrapper! {
    pub struct LpPrint(ObjectSubclass<imp::LpPrint>)
        @extends gtk::Widget, gtk::Window, adw::Window,
        @implements gtk::Buildable, gtk::ConstraintTarget;
}

#[gtk::template_callbacks]
impl LpPrint {
    pub fn new(
        image: LpImage,
        parent_window: gtk::Window,
        print_settings: Option<gtk::PrintSettings>,
        page_setup: Option<gtk::PageSetup>,
    ) -> Self {
        let print_settings = print_settings.unwrap_or_default();

        let obj: Self = glib::Object::builder()
            .property("image", image)
            .property("print-settings", print_settings)
            .property("parent-window", parent_window)
            .build();

        if let Some(page_setup) = page_setup {
            obj.print_operation()
                .set_default_page_setup(Some(&page_setup));
        }

        obj
    }

    fn ui_updates(&self) -> bool {
        !self.imp().ui_updates.disabled.get()
    }

    fn disable_ui_updates(&self) -> UiUpdates {
        self.imp().ui_updates.disable()
    }

    fn size_unit(&self) -> Unit {
        Unit::from(
            self.imp()
                .size_unit
                .selected_item()
                .and_downcast::<gtk::StringObject>()
                .map(|x| x.string())
                .unwrap_or_default(),
        )
    }

    fn margin_unit(&self) -> Unit {
        Unit::from(
            self.imp()
                .margin_unit
                .selected_item()
                .and_downcast::<gtk::StringObject>()
                .map(|x| x.string())
                .unwrap_or_default(),
        )
    }

    pub fn dpi(&self) -> f64 {
        self.print_settings().resolution() as f64
    }

    pub fn on_width_changed(&self) {
        if self.ui_updates() {
            let _ui_updates_disabled = self.disable_ui_updates();
            // Disable 'fill space' when manually changing size
            self.imp().fill_space.set_active(false);
            self.update_height();
            self.draw_preview();
        }
    }

    pub fn on_height_changed(&self) {
        if self.ui_updates() {
            let imp = self.imp();
            let _ui_updates_disabled = self.disable_ui_updates();
            // Disable 'fill space' when manually changing size
            imp.fill_space.set_active(false);

            let (orig_width, orig_height) = self.image().image_size();
            let width = self
                .size_unit()
                .round(imp.height.value() * orig_width as f64 / orig_height as f64);
            imp.width.set_value(width);

            self.draw_preview();
        }
    }

    fn user_alignment(&self) -> PageAlignment {
        let s = self
            .imp()
            .alignment
            .selected_item()
            .and_then(|x| x.downcast::<gtk::StringObject>().ok())
            .map(|x| x.string())
            .unwrap_or_default();

        PageAlignment::from(s.as_str())
    }

    /// Returns user selected image width in pixels
    pub fn user_width(&self) -> f64 {
        f64::max(1., self.imp().width.value() / self.width_unit_factor())
    }

    pub fn user_height(&self) -> f64 {
        let (_, orig_height) = self.image().image_size();

        f64::max(1., orig_height as f64 * self.user_scale())
    }

    /// Returns scaling of the original image based on selected image size
    pub fn user_scale(&self) -> f64 {
        let (orig_width, _) = self.image().image_size();

        self.user_width() / orig_width as f64
    }

    pub fn on_margin_horizontal_changed(&self) {
        if self.ui_updates() {
            let _ui_updates_disabled = self.disable_ui_updates();
            self.set_ranges();
            if self.fill_space() {
                self.update_width();
                self.update_height();
            }
            self.draw_preview();
        }
    }

    /// Returns user selected left margin in pixels
    pub fn user_margin_horizontal(&self) -> f64 {
        self.imp().margin_horizontal.value() / self.margin_unit_factor()
    }

    pub fn effective_margin_left(&self) -> f64 {
        match self.user_alignment().horizontal {
            HAlignment::Left => self.user_margin_horizontal(),
            HAlignment::Center => (self.paper_width() - self.user_width()) / 2.,
            HAlignment::Right => {
                self.paper_width() - self.user_width() - self.user_margin_horizontal()
            }
        }
    }

    pub fn paper_width(&self) -> f64 {
        self.page_setup().paper_width(gtk::Unit::Inch) * self.dpi()
    }

    pub fn paper_height(&self) -> f64 {
        self.page_setup().paper_height(gtk::Unit::Inch) * self.dpi()
    }

    pub fn on_margin_vertical_changed(&self) {
        if self.ui_updates() {
            let _ui_updates_disabled = self.disable_ui_updates();
            self.set_ranges();
            if self.fill_space() {
                let _ui_updates_disabled = self.disable_ui_updates();
                self.update_width();
                self.update_height();
            }
            self.draw_preview();
        }
    }

    /// Returns user selected top margin in pixels
    pub fn user_margin_vertical(&self) -> f64 {
        self.imp().margin_vertical.value() / self.margin_unit_factor()
    }

    pub fn effective_margin_top(&self) -> f64 {
        match self.user_alignment().vertical {
            VAlignment::Top => self.user_margin_vertical(),
            VAlignment::Middle => (self.paper_height() - self.user_height()) / 2.,
            VAlignment::Bottom => {
                self.paper_height() - self.user_height() - self.user_margin_vertical()
            }
        }
    }

    /// Redraws the preview widget child
    fn draw_preview(&self) {
        self.imp().preview.queue_resize();
    }

    /// Sets the allowed ranges for the spin button based on current unit
    fn set_ranges(&self) {
        let imp = self.imp();

        let margin_unit = self.margin_unit();
        let min_horizontal_margin = f64::max(
            self.page_setup().left_margin(gtk::Unit::Inch),
            self.page_setup().right_margin(gtk::Unit::Inch),
        ) * self.dpi();
        let min_vertical_margin = f64::max(
            self.page_setup().top_margin(gtk::Unit::Inch),
            self.page_setup().bottom_margin(gtk::Unit::Inch),
        ) * self.dpi();

        imp.margin_horizontal.set_range(
            margin_unit.ceil(min_horizontal_margin * self.margin_unit_factor()),
            margin_unit.floor((self.paper_width() / 2.) * self.margin_unit_factor()),
        );
        imp.margin_vertical.set_range(
            margin_unit.ceil(min_vertical_margin * self.margin_unit_factor()),
            margin_unit.floor((self.paper_height() / 2.) * self.margin_unit_factor()),
        );

        let size_unit = self.size_unit();
        let max_width = self.fill_space_width();
        let max_height = self.paper_height() - 2. * self.user_margin_vertical();

        imp.width.set_range(
            size_unit.ceil(self.width_unit_factor()),
            size_unit.floor(max_width * self.width_unit_factor()),
        );
        imp.height.set_range(
            size_unit.ceil(self.width_unit_factor()),
            size_unit.floor(max_height * self.height_unit_factor()),
        );
    }

    fn fill_space_width(&self) -> f64 {
        let width1 = self.paper_width() - 2. * self.user_margin_horizontal();

        let height = self.paper_height() - 2. * self.user_margin_vertical();
        let (orig_width, orig_height) = self.image().image_size();
        let width2 = height * orig_width as f64 / orig_height as f64;

        f64::min(width1, width2)
    }

    fn fill_space_height(&self) -> f64 {
        let (orig_width, orig_height) = self.image().image_size();

        self.fill_space_width() * orig_height as f64 / orig_width as f64
    }

    fn update_width(&self) {
        if self.fill_space() {
            let value = self
                .size_unit()
                .floor(self.fill_space_width() * self.width_unit_factor());
            self.imp().width.set_value(value);
        }
    }

    fn update_height(&self) {
        let height = if self.fill_space() {
            self.fill_space_height()
        } else {
            self.user_height()
        };

        let value = self.size_unit().round(height * self.height_unit_factor());
        self.imp().height.set_value(value);
    }

    fn on_size_unit_changed(&self) {
        let imp = self.imp();
        let _ui_updates_disabled = self.disable_ui_updates();

        let (orig_width, orig_height) = self.image().image_size();

        let unit = self.size_unit();
        let width_factor = unit.factor(self.dpi(), orig_width);
        let height_factor = unit.factor(self.dpi(), orig_height);

        let width = imp.width.value() * width_factor / self.width_unit_factor();
        let height = imp.height.value() * height_factor / self.height_unit_factor();

        self.set_width_unit_factor(width_factor);
        self.set_height_unit_factor(height_factor);

        let _ui_updates_disabled = self.disable_ui_updates();
        self.set_ranges();

        imp.width.set_digits(unit.digits());
        imp.width.set_value(width);

        imp.height.set_digits(unit.digits());
        imp.height.set_value(height);

        self.draw_preview();
    }

    fn on_margin_unit_changed(&self) {
        let imp = self.imp();
        let _ui_updates_disabled = self.disable_ui_updates();

        let unit = self.margin_unit();
        let unit_factor = unit.factor(self.dpi(), 1);

        let update_factor = unit_factor / self.margin_unit_factor();
        let margin_horizontal = imp.margin_horizontal.value() * update_factor;
        let margin_vertical = imp.margin_vertical.value() * update_factor;

        self.set_margin_unit_factor(unit_factor);
        self.set_ranges();

        imp.margin_horizontal.set_digits(unit.digits());
        imp.margin_horizontal.set_value(margin_horizontal);

        imp.margin_vertical.set_digits(unit.digits());
        imp.margin_vertical.set_value(margin_vertical);

        self.draw_preview();
    }

    pub fn fill_space(&self) -> bool {
        self.imp().fill_space.is_active()
    }

    fn on_fill_space_changed(&self) {
        if self.fill_space() {
            let _ui_updates_disabled = self.disable_ui_updates();
            self.update_width();
            self.update_height();
            self.draw_preview();
        }
    }

    pub fn run(&self) {
        self.print_operation()
            .run(
                gtk::PrintOperationAction::PrintDialog,
                self.imp().parent_window.get(),
            )
            .unwrap();
    }

    pub fn page_setup(&self) -> gtk::PageSetup {
        self.print_operation().default_page_setup()
    }

    /// Go back to print dialog from image layout
    async fn back(&self) {
        self.close();
        let print = Self::new(
            self.image(),
            self.parent_window(),
            self.print_operation().print_settings(),
            Some(self.print_operation().default_page_setup()),
        );
        print.run();
    }

    /// Initialize actual print operation
    fn print(&self) {
        self.close();

        let print_operation = gtk::PrintOperation::new();

        let print_settings = self.print_operation().print_settings();

        if let Some(print_settings) = &print_settings {
            if let Some(uri) = self.image().file().map(|x| x.uri()) {
                print_settings.set(gtk::PRINT_SETTINGS_OUTPUT_URI, Some(&format!("{uri}.pdf")));
            }
        }

        print_operation.set_print_settings(print_settings.as_ref());
        print_operation.set_default_page_setup(Some(&self.print_operation().default_page_setup()));

        print_operation.connect_begin_print(move |op, _ctx| {
            op.set_n_pages(1);
        });

        print_operation.connect_draw_page(
            glib::clone!(@weak self as obj => move |_operation, context, _page_nr| {
            obj.draw_page(context);
            }),
        );

        print_operation
            .run(
                gtk::PrintOperationAction::Print,
                Some(&self.parent_window()),
            )
            .unwrap();
    }

    /// Draw PDF for printing
    fn draw_page(&self, print_context: &gtk::PrintContext) {
        log::debug!("Drawing image to print");
        let image = self.image();

        let cairo_dpi = print_context.dpi_x();

        let (orig_width, orig_height) = image.image_size();

        let texture_scale = self.user_width() / orig_width as f64;
        let cairo_scale = cairo_dpi / self.dpi();

        let cairo_surface = if image.format().map_or(false, |x| x.is_svg()) {
            // Render SVG to exact needed sizes
            // TODO: This should be async
            decoder::formats::Svg::render_print(
                &image.file().unwrap(),
                (orig_width as f64 * texture_scale) as i32,
                (orig_height as f64 * texture_scale) as i32,
            )
            .unwrap()
        } else {
            let texture = image.print_data(texture_scale).unwrap();

            let mut downloader = gdk::TextureDownloader::new(&texture);
            downloader.set_format(gdk::MemoryFormat::B8g8r8a8Premultiplied);

            let (data, stride) = downloader.download_bytes();
            let data = data.to_vec();

            let width = texture.width();
            let height = texture.height();

            cairo::ImageSurface::create_for_data(
                data,
                cairo::Format::ARgb32,
                width,
                height,
                stride as i32,
            )
            .unwrap()
        };

        let cairo_context = print_context.cairo_context();
        cairo_context.scale(cairo_scale, cairo_scale);

        let margin_left = self.effective_margin_left()
            - self.page_setup().left_margin(gtk::Unit::Inch) * self.dpi();
        let margin_top = self.effective_margin_top()
            - self.page_setup().top_margin(gtk::Unit::Inch) * self.dpi();

        cairo_context
            .set_source_surface(cairo_surface, margin_left, margin_top)
            .unwrap();

        cairo_context.paint().unwrap();
    }
}

trait AdwSpinRowExt {
    fn set_range(&self, min: f64, max: f64);
}

use glib::translate::*;

impl AdwSpinRowExt for adw::SpinRow {
    fn set_range(&self, min: f64, max: f64) {
        unsafe {
            adw_spin_row_set_range(self.to_glib_none().0, min, max);
        }
    }
}

#[link(name = "adwaita-1")]
extern "C" {
    pub fn adw_spin_row_set_range(
        self_: *mut adw::ffi::AdwSpinRow,
        min: std::ffi::c_double,
        max: std::ffi::c_double,
    );
}
