// image_page.rs
//
// Copyright 2022 Christopher Davis <christopherdavis@gnome.org>
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

use gtk_macros::spawn;

use once_cell::sync::OnceCell;

use crate::widgets::LpImage;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Loupe/gtk/image_page.ui")]
    pub struct LpImagePage {
        #[template_child]
        pub(super) stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub(super) spinner: TemplateChild<gtk::Spinner>,
        #[template_child]
        pub(super) error_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub(super) image: TemplateChild<LpImage>,
        #[template_child]
        pub(super) popover: TemplateChild<gtk::PopoverMenu>,
        #[template_child]
        pub(super) click_gesture: TemplateChild<gtk::GestureClick>,
        #[template_child]
        pub(super) press_gesture: TemplateChild<gtk::GestureLongPress>,

        pub(super) file: OnceCell<gio::File>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LpImagePage {
        const NAME: &'static str = "LpImagePage";
        type Type = super::LpImagePage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LpImagePage {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            self.click_gesture
                .connect_pressed(clone!(@weak obj => move |gesture, _, x, y| {
                    obj.show_popover_at(x, y);
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                }));

            self.press_gesture
                .connect_pressed(clone!(@weak obj => move |gesture, x, y| {
                    log::debug!("Long press triggered");
                    obj.show_popover_at(x, y);
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                }));
        }
    }

    impl WidgetImpl for LpImagePage {}
    impl BinImpl for LpImagePage {}
}

glib::wrapper! {
    pub struct LpImagePage(ObjectSubclass<imp::LpImagePage>)
        @extends gtk::Widget, adw::Bin,
        @implements gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl LpImagePage {
    pub fn from_file(file: &gio::File) -> Self {
        let obj = glib::Object::new::<Self>(&[]).unwrap();
        obj.imp().file.set(file.clone()).unwrap();

        // This doesn't work properly for items not explicitly selected
        // via the file chooser portal. I'm not sure how to make this work.
        gtk::RecentManager::default().add_item(&file.uri());

        spawn!(clone!(@weak obj, @weak file => async move {
            let imp = obj.imp();
            match load_texture_from_file(&file).await {
                Ok(texture) => {
                    imp.image.set_texture_with_file(texture, &file);
                    imp.stack.set_visible_child(&*imp.image);
                    imp.spinner.set_spinning(false);
                },
                Err(e) => {
                    imp.stack.set_visible_child(&*imp.error_page);
                    imp.spinner.set_spinning(false);
                    log::error!("Could not load image: {e}");
                }
            }
        }));

        obj
    }

    pub fn file(&self) -> Option<gio::File> {
        self.imp().file.get().cloned()
    }

    pub fn texture(&self) -> Option<gdk::Texture> {
        self.imp().image.texture()
    }

    pub fn content_provider(&self) -> gdk::ContentProvider {
        self.imp().image.content_provider()
    }

    pub fn show_popover_at(&self, x: f64, y: f64) {
        let imp = self.imp();

        let rect = gdk::Rectangle::new(x as i32, y as i32, 0, 0);

        imp.popover.set_pointing_to(Some(&rect));
        imp.popover.popup();
    }
}

async fn load_texture_from_file(file: &gio::File) -> Result<gdk::Texture, glib::Error> {
    let (sender, receiver) = futures_channel::oneshot::channel();

    let _ = std::thread::Builder::new()
        .name("Load Texture".to_string())
        .spawn(clone!(@weak file => move || {
            let result = gdk::Texture::from_file(&file);
            sender.send(result).unwrap()
        }));

    receiver.await.unwrap()
}