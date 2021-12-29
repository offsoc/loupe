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

use crate::deps::*;

use adw::subclass::prelude::*;
use glib::clone;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

use crate::config;
use crate::util;
use crate::widgets::LpImageView;

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
    //
    // Because all of our member fields implement the
    // `Default` trait, we can use `#[derive(Default)]`.
    // If some member fields did not implement default,
    // we'd need to have a `new()` function in the
    // `impl ObjectSubclass for $TYPE` section.
    #[derive(Default, Debug, CompositeTemplate)]
    #[template(resource = "/org/gnome/Loupe/gtk/window.ui")]
    pub struct LpWindow {
        // Template children are used with the
        // TemplateChild<T> wrapper, where T is the
        // object type of the template child.
        #[template_child]
        pub flap: TemplateChild<adw::Flap>,
        #[template_child]
        pub headerbar: TemplateChild<gtk::HeaderBar>,
        #[template_child]
        pub menu_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub fullscreen_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub status_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub image_view: TemplateChild<LpImageView>,
        #[template_child]
        pub drop_target: TemplateChild<gtk::DropTarget>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LpWindow {
        const NAME: &'static str = "LpWindow";
        type Type = super::LpWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            // bind_template() is a function generated by the
            // CompositeTemplate macro to bind all children at once.
            Self::bind_template(klass);

            // Set up actions
            klass.install_action("win.toggle-fullscreen", None, move |win, _, _| {
                win.toggle_fullscreen(!win.is_fullscreened());
            });

            klass.install_action("win.open", None, move |win, _, _| {
                win.pick_file();
            });

            klass.install_action("win.open-with", None, move |win, _, _| {
                win.open_with();
            });

            klass.install_action("win.set-wallpaper", None, move |win, _, _| {
                win.set_wallpaper();
            });

            klass.install_action("win.print", None, move |win, _, _| {
                win.print();
            });

            klass.install_action("win.show-toast", Some("(si)"), move |win, _, var| {
                if let Some((ref toast, i)) = var.map(|v| v.get::<(String, i32)>()).flatten() {
                    win.show_toast(toast, adw::ToastPriority::__Unknown(i));
                }
            });
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LpWindow {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            if config::PROFILE == ".Devel" {
                obj.add_css_class("devel");
            }

            obj.set_actions_enabled(false);

            self.status_page
                .set_icon_name(Some(&format!("{}-symbolic", config::APP_ID)));

            // Set help overlay
            let builder = gtk::Builder::from_resource("/org/gnome/Loupe/gtk/help_overlay.ui");
            let help_overlay = builder.object("help_overlay").unwrap();
            obj.set_help_overlay(Some(&help_overlay));

            // For callbacks, you will want to reference the GTK docs on
            // the relevant signal to see which parameters you need.
            // In this case, we need one to react to the signal,
            // so we name it `iv` then use `_` for the other spots.
            self.image_view.connect_notify_local(
                Some("filename"),
                clone!(@weak obj => move |iv, _| {
                    if let Some(filename) = iv.filename() {
                        obj.set_title(Some(&filename));
                    }
                }),
            );

            self.drop_target.set_types(&[gdk::FileList::static_type()]);
            self.drop_target.connect_drop(
                clone!(@weak obj => @default-return false, move |_, value, _, _| {
                    // Here we use a GValue, which is a dynamic object that can hold different types,
                    // e.g. strings, numbers, or in this case objects. In order to get the GdkFileList
                    // from the GValue, we need to use the `get()` method.
                    //
                    // We've added type annotations here, and written it as `let list: gdk::FileList = ...`,
                    // but you might also see places where type arguments are used.
                    // This line could have been written as `let list = value.get::<gdk::FileList>().unwrap()`.
                    let list: gdk::FileList = value.get().unwrap();

                    // TODO: Handle this like EOG and make a "directory" out of the given files
                    let file = list.files().get(0).unwrap().clone();
                    let info = util::query_attributes(
                        &file,
                        vec![
                            &gio::FILE_ATTRIBUTE_STANDARD_CONTENT_TYPE,
                            &gio::FILE_ATTRIBUTE_STANDARD_DISPLAY_NAME,
                        ],
                    )
                    .expect("Could not query file info");

                    if info
                        .content_type()
                        .map(|t| t.to_string())
                        .filter(|t| t.starts_with("image/"))
                        .is_some() {
                        obj.set_image_from_file(&file, false);
                    } else {
                        obj.show_toast(
                            &format!("\"{}\" is not a valid image.", info.display_name().to_string()),
                            adw::ToastPriority::High,
                        );
                    }

                    true
                }),
            );
        }
    }

    impl WidgetImpl for LpWindow {}
    impl WindowImpl for LpWindow {}
    impl ApplicationWindowImpl for LpWindow {}
    impl AdwApplicationWindowImpl for LpWindow {}
}

glib::wrapper! {
    pub struct LpWindow(ObjectSubclass<imp::LpWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,
        @implements gio::ActionMap, gio::ActionGroup;
}

impl LpWindow {
    pub fn new<A: IsA<gtk::Application>>(app: &A) -> Self {
        glib::Object::new(&[("application", app)]).expect("Failed to create LpWindow")
    }

    fn toggle_fullscreen(&self, fullscreen: bool) {
        let imp = self.imp();

        if fullscreen {
            imp.flap.set_fold_policy(adw::FlapFoldPolicy::Always);
            imp.headerbar.add_css_class("osd");
            imp.menu_button.unparent();
            imp.fullscreen_button.unparent();
            imp.headerbar.pack_end(&*imp.fullscreen_button);
            imp.headerbar.pack_end(&*imp.menu_button);
            imp.fullscreen_button.set_icon_name("view-restore-symbolic");
        } else {
            imp.flap.set_fold_policy(adw::FlapFoldPolicy::Never);
            imp.headerbar.remove_css_class("osd");
            imp.menu_button.unparent();
            imp.fullscreen_button.unparent();
            imp.headerbar.pack_end(&*imp.menu_button);
            imp.headerbar.pack_end(&*imp.fullscreen_button);
            imp.fullscreen_button
                .set_icon_name("view-fullscreen-symbolic");
        }

        self.set_fullscreened(fullscreen);
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
        filter.set_property("name", &String::from("Supported image files"));
        filter.add_mime_type("image/*");
        chooser.add_filter(&filter);

        chooser.connect_response(
            clone!(@weak self as win, @strong chooser => move |_, resp| {
                if resp == gtk::ResponseType::Accept {
                    if let Some(file) = chooser.file() {
                        win.set_image_from_file(&file, true);
                    }
                }
            }),
        );

        chooser.show();
    }

    fn open_with(&self) {
        let imp = self.imp();

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
        let imp = self.imp();

        if let Err(e) = imp.image_view.set_wallpaper() {
            log::error!("Failed to set wallpaper: {}", e);
        }
    }

    fn print(&self) {
        let imp = self.imp();

        if let Err(e) = imp.image_view.print() {
            log::error!("Failed to print file: {}", e);
        }
    }

    fn show_toast(&self, text: &impl AsRef<str>, priority: adw::ToastPriority) {
        let imp = self.imp();

        let toast = adw::Toast::new(text.as_ref());
        toast.set_priority(priority);

        imp.toast_overlay.add_toast(&toast);
    }

    pub fn set_image_from_file(&self, file: &gio::File, resize: bool) {
        let imp = self.imp();

        log::debug!("Loading file: {}", file.uri().to_string());
        match imp.image_view.set_image_from_file(file) {
            Ok((width, height)) => {
                if resize {
                    self.resize_from_dimensions(width, height);
                }

                imp.stack.set_visible_child(&*imp.image_view);
                imp.image_view.grab_focus();
                self.set_actions_enabled(true)
            }
            Err(e) => log::error!("Could not load file: {}", e.to_string()),
        }
    }

    pub fn set_actions_enabled(&self, enabled: bool) {
        self.action_set_enabled("win.open-with", enabled);
        self.action_set_enabled("win.set-wallpaper", enabled);
        self.action_set_enabled("win.toggle-fullscreen", enabled);
        self.action_set_enabled("win.print", enabled);
    }

    // Adapted from https://gitlab.gnome.org/GNOME/eog/-/blob/master/src/eog-window.c:eog_window_obtain_desired_size
    pub fn resize_from_dimensions(&self, img_width: i32, img_height: i32) {
        let imp = self.imp();
        let mut final_width = img_width;
        let mut final_height = img_height;

        let header_height = imp.headerbar.height();

        // Ensure the window surface exists
        if !self.is_realized() {
            self.realize();
        }

        let display = gdk::Display::default().unwrap();
        let monitor = display
            .monitor_at_surface(&self.native().unwrap().surface().unwrap())
            .unwrap();
        let monitor_geometry = monitor.geometry();

        let monitor_width = monitor_geometry.width();
        let monitor_height = monitor_geometry.height();

        if img_width > monitor_width || img_height + header_height > monitor_height {
            let width_factor = (monitor_width as f32 * 0.85) / img_width as f32;
            let height_factor =
                (monitor_height as f32 * 0.85 - header_height as f32) / img_height as f32;
            let factor = width_factor.min(height_factor);

            final_width = (final_width as f32 * factor).round() as i32;
            final_height = (final_height as f32 * factor).round() as i32;
        }

        self.set_default_size(final_width, final_height);
        log::debug!("Window resized to {} x {}", final_width, final_height);
    }
}
