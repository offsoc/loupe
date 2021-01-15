// window.rs
//
// Copyright 2020 Christopher Davis <christopherdavis@gnome.org>
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

use gio::prelude::*;
use glib::clone;
use glib::subclass::prelude::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use gtk_macros::*;
use libadwaita::subclass::prelude::*;

use crate::config;
use crate::widgets::IvImageView;

mod imp {
    use super::*;

    // To use composite templates, you need
    // to use derive macro. Derive macros generate
    // code to e.g. implement a trait on something.
    // In this case, code is generated for Debug output
    // and to handle binding the template children.
    //
    // For this derive macro, you need to have
    // `use gtk::CompositeTemplate` in your code.
    #[derive(Debug, CompositeTemplate)]
    pub struct IvWindow {
        // Template children are used with the
        // TemplateChild<T> wrapper, where T is the
        // object type of the template child.
        #[template_child]
        pub headerbar: TemplateChild<gtk::HeaderBar>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub status_page: TemplateChild<libadwaita::StatusPage>,
        #[template_child]
        pub image_view: TemplateChild<IvImageView>,
        #[template_child]
        pub open_gesture: TemplateChild<gtk::GestureClick>,
    }

    impl ObjectSubclass for IvWindow {
        const NAME: &'static str = "IvWindow";
        type Type = super::IvWindow;
        type ParentType = libadwaita::ApplicationWindow;
        type Instance = glib::subclass::simple::InstanceStruct<Self>;
        type Class = glib::subclass::simple::ClassStruct<Self>;

        glib::object_subclass!();

        fn new() -> Self {
            Self {
                // For the initial value, use this function.
                headerbar: TemplateChild::default(),
                stack: TemplateChild::default(),
                status_page: TemplateChild::default(),
                image_view: TemplateChild::default(),
                open_gesture: TemplateChild::default(),
            }
        }

        fn class_init(klass: &mut Self::Class) {
            klass.set_template_from_resource("/org/gnome/ImageViewer/gtk/window.ui");
            // bind_template_children() is a function generated by the
            // CompositeTemplate macro to bind all children at once.
            Self::bind_template_children(klass);
        }
        fn instance_init(obj: &glib::subclass::InitializingObject<Self::Type>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for IvWindow {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            if config::PROFILE == ".Devel" {
                obj.add_css_class("devel");
            }

            obj.setup_actions();

            self.status_page
                .set_icon_name(Some(&format!("{}-symbolic", config::APP_ID)));

            // Set help overlay
            let builder = gtk::Builder::from_resource("/org/gnome/ImageViewer/gtk/help_overlay.ui");
            let help_overlay = builder.get_object("help_overlay").unwrap();
            obj.set_help_overlay(Some(&help_overlay));

            // For callbacks, you will want to reference the GTK docs on
            // the relevant signal to see which parameters you need.
            // In this case, we need none to react to the gesture,
            // so "_" is used in all 4 spots.
            self.open_gesture
                .connect_released(clone!(@weak obj => move |_, _, _, _| {
                    obj.pick_file();
                }));

            obj.bind_property("fullscreened", &*self.image_view, "header-visible")
                .flags(glib::BindingFlags::SYNC_CREATE | glib::BindingFlags::BIDIRECTIONAL)
                .build()
                .unwrap();
            obj.bind_property("fullscreened", &*self.headerbar, "visible")
                .flags(
                    glib::BindingFlags::SYNC_CREATE
                        | glib::BindingFlags::BIDIRECTIONAL
                        | glib::BindingFlags::INVERT_BOOLEAN,
                )
                .build()
                .unwrap();
        }
    }

    impl WidgetImpl for IvWindow {}
    impl WindowImpl for IvWindow {}
    impl ApplicationWindowImpl for IvWindow {}
    impl AdwApplicationWindowImpl for IvWindow {}
}

glib::wrapper! {
    pub struct IvWindow(ObjectSubclass<imp::IvWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, libadwaita::ApplicationWindow,
        @implements gio::ActionMap, gio::ActionGroup;
}

impl IvWindow {
    pub fn new<A: IsA<gtk::Application>>(app: &A) -> Self {
        glib::Object::new(&[("application", app)]).expect("Failed to create IvWindow")
    }

    pub fn setup_actions(&self) {
        action!(
            self,
            "toggle-fullscreen",
            clone!(@weak self as win => move |_, _| {
                win.set_property_fullscreened(!win.get_property_fullscreened());
            })
        );

        action!(
            self,
            "open",
            clone!(@weak self as win => move |_, _| {
                win.pick_file();
            })
        );

        action!(
            self,
            "open-with",
            clone!(@weak self as win => move |_, _| {
                win.open_with();
            })
        );

        action!(
            self,
            "set-wallpaper",
            clone!(@weak self as win => move |_, _| {
                win.set_wallpaper();
            })
        );

        action!(
            self,
            "print",
            clone!(@weak self as win => move |_, _| {
                win.print();
            })
        );

        self.set_actions_enabled(false);
    }

    fn pick_file(&self) {
        let chooser = gtk::FileChooserNative::new(
            Some(&"Open Image".to_string()),
            Some(self),
            gtk::FileChooserAction::Open,
            None,
            None,
        );

        chooser.set_modal(true);
        chooser.set_transient_for(Some(self));

        let filter = gtk::FileFilter::new();
        filter
            .set_property("name", &String::from("Supported image files"))
            .unwrap();
        filter.add_mime_type("image/*");
        chooser.add_filter(&filter);

        chooser.connect_response(
            clone!(@weak self as win, @strong chooser => move |_, resp| {
                if resp == gtk::ResponseType::Accept {
                    if let Some(file) = chooser.get_file() {
                        win.set_image_from_file(&file);
                    }
                }
            }),
        );

        chooser.show();
    }

    fn open_with(&self) {
        let imp = imp::IvWindow::from_instance(self);

        if let Some(uri) = imp.image_view.uri() {
            std::process::Command::new("xdg-open")
                .arg(uri)
                .output()
                .unwrap();
        } else {
            log::error!("No URI for current image.")
        }
    }

    fn set_wallpaper(&self) {
        let imp = imp::IvWindow::from_instance(self);

        if let Err(e) = imp.image_view.set_wallpaper() {
            log::error!("Failed to set wallpaper: {}", e);
        }
    }

    fn print(&self) {
        let imp = imp::IvWindow::from_instance(self);

        if let Err(e) = imp.image_view.print() {
            log::error!("Failed to print file: {}", e);
        }
    }

    pub fn set_image_from_file(&self, file: &gio::File) {
        let imp = imp::IvWindow::from_instance(self);

        imp.image_view.set_image_from_file(file);
        imp.stack.set_visible_child(&*imp.image_view);

        log::debug!("Loading file: {}", file.get_uri().to_string());

        if let Some(name) = file.get_basename() {
            self.set_title(Some(
                name.file_name().expect("Missing name").to_str().unwrap(),
            ));
        }

        self.set_actions_enabled(true);
    }

    pub fn set_actions_enabled(&self, enabled: bool) {
        // Here we need to downcast because lookup_action()` returns a plain
        // GAction, not a GSimpleAction.
        self.lookup_action("open-with")
            .unwrap()
            .downcast::<gio::SimpleAction>()
            .unwrap()
            .set_enabled(enabled);
        self.lookup_action("set-wallpaper")
            .unwrap()
            .downcast::<gio::SimpleAction>()
            .unwrap()
            .set_enabled(enabled);
        self.lookup_action("toggle-fullscreen")
            .unwrap()
            .downcast::<gio::SimpleAction>()
            .unwrap()
            .set_enabled(enabled);
        self.lookup_action("print")
            .unwrap()
            .downcast::<gio::SimpleAction>()
            .unwrap()
            .set_enabled(enabled);
    }
}
