use gio::prelude::*;
use glib::clone;
use glib::Receiver;
use glib::Sender;
use glib::{ToValue, Value};
use gtk::prelude::*;
use qubes_app_linux_converter_client_core::{
    convert_all_files, default_archive_folder, ConvertEvent, ConvertParameters,
};
use std::fs::File;
use std::io::prelude::*;
use std::{collections::HashMap, convert::TryInto, fs, sync::mpsc, thread};

fn main() {
    env_logger::init();
    let (transmit_gtk_transmitter, receive_gtk_transmitter) = std::sync::mpsc::channel();
    let (ui_to_controller_transmitter, ui_to_controller_receiver) = std::sync::mpsc::channel();

    thread::spawn(move || {
        let controller_to_ui_transmitter: glib::Sender<ConvertEvent> =
            receive_gtk_transmitter.recv().unwrap();
        let parameters = ui_to_controller_receiver.recv().unwrap();
        let (backend_to_controller_transmitter, backend_to_controller_receiver) =
            std::sync::mpsc::channel();
        thread::spawn(move || {
            convert_all_files(&backend_to_controller_transmitter, &parameters).unwrap();
        });
        for event in backend_to_controller_receiver {
            controller_to_ui_transmitter.send(event).unwrap();
        }
    });
    let application =
        gtk::Application::new(Some("Qubes.converter"), gio::ApplicationFlags::default())
            .expect("Initialization failed...");
    application.connect_activate(move |_| {
        let (controller_to_ui_transmitter, controller_to_ui_receiver) =
            glib::MainContext::channel(glib::PRIORITY_DEFAULT);
        transmit_gtk_transmitter
            .send(controller_to_ui_transmitter)
            .unwrap();
        build_ui(
            controller_to_ui_receiver,
            ui_to_controller_transmitter.clone(),
        );
    });
    application.run(&[]);
}

fn build_ui(
    data_to_ui: Receiver<ConvertEvent>,
    data_from_ui: std::sync::mpsc::Sender<ConvertParameters>,
) {
    let glade_src = include_str!("../window.glade");
    let builder = gtk::Builder::from_string(glade_src);
    let define_parameters_window: gtk::Window =
        builder.get_object("define_parameters_window").unwrap();
    let follow_convert_status_window: gtk::Window =
        builder.get_object("follow_convert_status_window").unwrap();
    let convert_status_liststore: gtk::ListStore =
        builder.get_object("convert_status_liststore").unwrap();
    define_parameters_window.show();
    let file_picker: gtk::FileChooserButton = builder.get_object("files").unwrap();
    let archive_folder: gtk::FileChooserButton = builder.get_object("archive_folder").unwrap();
    let in_place: gtk::CheckButton = builder.get_object("in_place").unwrap();

    in_place.set_active(false);
    archive_folder.set_current_folder_uri(&default_archive_folder());

    let launch_button: gtk::Button = builder.get_object("start").unwrap();
    follow_convert_status_window.connect_destroy(
        clone!(@weak follow_convert_status_window => move|_|{
            gtk::main_quit();
        }),
    );
    define_parameters_window.connect_destroy(
        clone!(@weak define_parameters_window => move|_|{
            gtk::main_quit();
        }),
    );

    launch_button.connect_clicked(clone!(@weak define_parameters_window => move |_|{
      let mut files = Vec::new();
      let files_vec = file_picker.get_filenames();
      if files_vec.is_empty(){
          return;
      }
      for file_gtk in files_vec{
          files.push(file_gtk.to_str().unwrap().to_string());
      }
      data_from_ui.send(ConvertParameters{in_place: in_place.get_active(), archive: Some(match archive_folder.get_current_folder_uri(){
          Some(uri) => format!("{}/",uri),
          None => default_archive_folder()
      }), files}).unwrap();
      define_parameters_window.close();
      follow_convert_status_window.show();
    }));

    data_to_ui.attach(
          None,
          clone!(@weak convert_status_liststore => @default-return Continue(true), move |convert_event| {
              update_convert_status_gui(
                  &convert_event,
                  &convert_status_liststore,
              )
          }),
      );
    gtk::main();
}

fn update_convert_status_gui(
    convert_event: &ConvertEvent,
    model: &gtk::ListStore,
) -> glib::Continue {
    match convert_event {
        ConvertEvent::FileInfo {
            output_type,
            number_pages,
            file,
        } => {
            let gtk_number_pages = *number_pages as u32;
            let gtk_current_page: u32 = 0;
            let gtk_percentage_progress: f32 = 0.0;
            let values: [&dyn ToValue; 5] = [
                &file,
                &gtk_number_pages,
                &gtk_current_page,
                &gtk_percentage_progress,
                &format!("Ongoing"),
            ];
            let col_indices: [u32; 5] = [0, 1, 2, 3, 4];
            model.set(&model.append(), &col_indices, &values);
        }
        ConvertEvent::PageConverted { file, page } => {
            model.foreach(|tree_model, tree_path, tree_iter| {
                let gtk_filename: String = model
                    .get_value(&tree_iter, 0)
                    .get::<String>()
                    .unwrap()
                    .unwrap();
                if &gtk_filename == file {
                    let total_page = model
                        .get_value(&tree_iter, 1)
                        .get::<u32>()
                        .unwrap()
                        .unwrap();
                    let gtk_page = *page as u32;
                    let gtk_value = gtk_page.to_value();
                    let percentage: f32 = (*page as f32) * 100.0 / (total_page as f32);
                    let gtk_percentage = percentage.to_value();
                    model.set_value(&tree_iter, 2, &gtk_value);
                    model.set_value(&tree_iter, 3, &gtk_percentage);
                    return false;
                }
                return true;
            });
        }
        ConvertEvent::FileConverted { file } => {
            model.foreach(|tree_model, tree_path, tree_iter| {
                let gtk_filename: String = model
                    .get_value(&tree_iter, 0)
                    .get::<String>()
                    .unwrap()
                    .unwrap();
                if &gtk_filename == file {
                    let gtk_status = "Done".to_value();
                    let percentage: f32 = 100.0;
                    let gtk_percentage = percentage.to_value();
                    model.set_value(&tree_iter, 3, &gtk_percentage);
                    model.set_value(&tree_iter, 4, &gtk_status);
                    return false;
                }
                return true;
            });
        }
        ConvertEvent::Failure { file, message } => {
            model.foreach(|tree_model, tree_path, tree_iter| {
                let gtk_filename: String = model
                    .get_value(&tree_iter, 0)
                    .get::<String>()
                    .unwrap()
                    .unwrap();
                if &gtk_filename == file {
                    let gtk_status = "Failure".to_value();
                    model.set_value(&tree_iter, 4, &gtk_status);
                    return false;
                }
                true
            });
        }
    }
    Continue(true)
}
