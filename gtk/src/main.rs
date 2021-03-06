#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]
use gio::prelude::*;
use qubes_converter_client::{
    convert_all_files, default_archive_folder, list_ocr_langs, ConvertEvent, ConvertParameters,
};

use clap::Parser;
use glib::{clone, GString, Receiver, ToValue};
use glob::glob;
use gtk4::prelude::*;
use log::debug;
use std::{fs, thread};

//#[clap(setting = AppSettings::ColoredHelp)]
#[derive(Parser)]
#[clap(version, author)]
struct Opts {
    files: Vec<String>,
}

fn main() {
    env_logger::init();
    let opts: Opts = Opts::parse();
    let mut all_files = Vec::new();
    for file in opts.files {
        for entry in glob(&file).expect("Failed to read glob pattern") {
            let path = entry.expect("glob error");
            all_files.push(
                fs::canonicalize(path)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string(),
            );
        }
    }
    let ocr_languages = match list_ocr_langs() {
        Err(_) => Vec::new(),
        Ok(langs) => langs,
    };
    let (transmit_gtk_transmitter, receive_gtk_transmitter) = std::sync::mpsc::channel();
    let (ui_to_controller_transmitter, ui_to_controller_receiver) = std::sync::mpsc::channel();

    debug!("Spawning data thread");
    thread::spawn(move || {
        let controller_to_ui_transmitter: glib::Sender<ConvertEvent> =
            receive_gtk_transmitter.recv().unwrap();
        let parameters = ui_to_controller_receiver.recv().unwrap();
        let (backend_to_controller_transmitter, backend_to_controller_receiver) =
            std::sync::mpsc::channel();
        thread::spawn(move || {
            convert_all_files(&backend_to_controller_transmitter, parameters).unwrap();
        });
        for event in backend_to_controller_receiver {
            controller_to_ui_transmitter.send(event).unwrap();
        }
    });
    // TODO somehow, GTK try to read the env::args and is not apply if it is not empty.
    // Need to find a way to tell GTK that the env::args are not his problem.
    debug!("Starting GTK");
    let application =
        gtk4::Application::new(Some("Qubes.converter"), gio::ApplicationFlags::default());
    application.connect_activate(move |application| {
        let (controller_to_ui_transmitter, controller_to_ui_receiver) =
            glib::MainContext::channel(glib::PRIORITY_DEFAULT);
        transmit_gtk_transmitter
            .send(controller_to_ui_transmitter)
            .unwrap();
        build_ui(
            application,
            controller_to_ui_receiver,
            ui_to_controller_transmitter.clone(),
            &all_files,
            &ocr_languages,
        );
    });
    application.run_with_args(&[""]);
}
fn connect_launch_button(
    archive_liststore: &gtk4::ListStore,
    files_liststore: &gtk4::ListStore,
    follow_convert_status_window: &gtk4::ApplicationWindow,
    define_parameters_window: &gtk4::ApplicationWindow,
    in_place: bool,
    default_password: String,
    data_from_ui: &std::sync::mpsc::Sender<ConvertParameters>,
    application: &gtk4::Application,
    ocr_language: Option<GString>,
) {
    debug!("Trying to start converting");
    let mut files = Vec::new();
    files_liststore.foreach(|_tree_model, _tree_path, tree_iter| {
        let gtk_filename: String = files_liststore.get(tree_iter, 0).get::<String>().unwrap();
        files.push(gtk_filename);
        false
    });
    let mut archive = None;
    archive_liststore.foreach(|_tree_model, _tree_path, tree_iter| {
        let archive_name: String = archive_liststore.get(tree_iter, 0).get::<String>().unwrap();
        archive = Some(archive_name);
        true
    });
    if files.is_empty() {
        return;
    }
    let ocr = ocr_language.map(|o| o.to_string());
    data_from_ui
        .send(ConvertParameters {
            in_place,
            default_password,
            archive: Some(match archive {
                Some(uri) => format!("{}/", uri),
                None => default_archive_folder(),
            }),
            files,
            max_pages_converted_in_parallele: 1,
            ocr,
            stderr: true,
        })
        .unwrap();
    follow_convert_status_window.set_application(Some(application));
    let no_application: Option<&gtk4::Application> = None;
    define_parameters_window.set_application(no_application);
    define_parameters_window.close();
    follow_convert_status_window.show();
}
fn connect_archive_folder_chooser_button(
    archive_liststore: &gtk4::ListStore,
    archive_folder_button: &gtk4::Button,
    window: &gtk4::ApplicationWindow,
) {
    debug!("Launching file picker to select archive folder");
    let file_chooser = gtk4::FileChooserNativeBuilder::new()
        .title("Archive folder")
        .transient_for(window)
        .action(gtk4::FileChooserAction::SelectFolder)
        .build();
    file_chooser.connect_response(
            clone!(@weak archive_liststore, @weak archive_folder_button ,@strong file_chooser => move |_, r| {
                if r == gtk4::ResponseType::Accept {
                    archive_liststore.clear();
                        let filename = &file_chooser.file().unwrap().path().unwrap().to_str().unwrap().to_string();
                        archive_liststore.set(&archive_liststore.append(), &[(0, &filename)]);
                    archive_folder_button.set_label(filename);
                }
            }),
        );
    file_chooser.show();
}
fn connect_files_chooser_button(
    files_liststore: &gtk4::ListStore,
    files_picker_button: &gtk4::Button,
    window: &gtk4::ApplicationWindow,
) {
    debug!("Launching file picker to select files to convert");
    let file_chooser = gtk4::FileChooserNativeBuilder::new()
        .title("Files to convert")
        .transient_for(window)
        .select_multiple(true)
        .action(gtk4::FileChooserAction::Open)
        .build();
    file_chooser.connect_response(
            clone!(@weak files_liststore, @weak files_picker_button, @strong file_chooser => move |_, r| {
                if r == gtk4::ResponseType::Accept {
                    let listmodel = file_chooser.files().unwrap();
                    let mut index = 0;
                    files_liststore.clear();
                    let mut button_label = String::new();
                    while let Some(file) = listmodel.item(index){
                        let gtkfile = file.downcast_ref::<gio::File>().unwrap();
                        let filename = gtkfile.path().unwrap().to_str().unwrap().to_string();
                        files_liststore.set(&files_liststore.append(), &[(0, &filename)]);
                        button_label.push_str(&filename);
                        button_label.push('\n');
                        index+=1;
                    }
                    files_picker_button.set_label(&button_label);
                }
            }),
        );
    file_chooser.show();
}
fn build_ui(
    application: &gtk4::Application,
    data_to_ui: Receiver<ConvertEvent>,
    data_from_ui: std::sync::mpsc::Sender<ConvertParameters>,
    files: &[String],
    ocr_languages: &[String],
) {
    debug!("reading ui files");
    let parameters_selection_builder =
        gtk4::Builder::from_string(include_str!("../gtk_ui/parameters_selection.ui"));
    let convert_status_progress_builder =
        gtk4::Builder::from_string(include_str!("../gtk_ui/convert_status_progress.ui"));

    debug!("Getting UI objects");
    let define_parameters_window: gtk4::ApplicationWindow = parameters_selection_builder
        .object("define_parameters_window")
        .unwrap();
    define_parameters_window.set_application(Some(application));
    let files_liststore: gtk4::ListStore = parameters_selection_builder
        .object("liststore_files")
        .unwrap();
    let archive_liststore: gtk4::ListStore = parameters_selection_builder
        .object("liststore_archive")
        .unwrap();
    let ocr_language_combo: gtk4::ComboBoxText =
        parameters_selection_builder.object("ocr_language").unwrap();
    let follow_convert_status_window: gtk4::ApplicationWindow = convert_status_progress_builder
        .object("follow_convert_status_window")
        .unwrap();
    let convert_status_liststore: gtk4::ListStore = convert_status_progress_builder
        .object("convert_status_liststore")
        .unwrap();
    let file_picker_button: gtk4::Button = parameters_selection_builder.object("files").unwrap();
    let archive_folder_button: gtk4::Button = parameters_selection_builder
        .object("archive_folder")
        .unwrap();
    let default_password: gtk4::Entry = parameters_selection_builder
        .object("default_password")
        .unwrap();
    archive_folder_button.set_label(&default_archive_folder());
    if !files.is_empty() {
        file_picker_button.set_label(&files.join("\n"));
        for file in files {
            files_liststore.set(&files_liststore.append(), &[(0, &file.as_str())]);
        }
    }
    for language in ocr_languages {
        ocr_language_combo.append_text(language.as_str());
    }
    let in_place: gtk4::CheckButton = parameters_selection_builder.object("in_place").unwrap();
    let launch_button: gtk4::Button = parameters_selection_builder.object("start").unwrap();

    debug!("Configuring UI events");

    launch_button.connect_clicked(clone!(@weak ocr_language_combo ,@weak files_liststore, @weak archive_liststore, @weak define_parameters_window, @weak application, @weak default_password => move |_|{
        connect_launch_button(&archive_liststore, &files_liststore, &follow_convert_status_window, &define_parameters_window, in_place.is_active(), default_password.text().to_string(), &data_from_ui, &application, ocr_language_combo.active_text());
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
    archive_folder_button.connect_clicked(clone!(@weak archive_folder_button, @weak define_parameters_window => move |_|{
         connect_archive_folder_chooser_button(&archive_liststore, &archive_folder_button, &define_parameters_window);
    }));
    file_picker_button.connect_clicked(
        clone!(@weak file_picker_button, @weak define_parameters_window => move |_|{
           connect_files_chooser_button(&files_liststore, &file_picker_button, &define_parameters_window);
        }),
    );
    debug!("Display Parameter Window");
    define_parameters_window.show();
}

fn update_convert_status_gui(
    convert_event: &ConvertEvent,
    model: &gtk4::ListStore,
) -> glib::Continue {
    match convert_event {
        ConvertEvent::FileToConvert { file } => {
            let gtk_number_pages: u32 = 0;
            let gtk_current_page: u32 = 0;
            let gtk_percentage_progress: f32 = 0.0;
            let values: [(u32, &dyn ToValue); 5] = [
                (0, &file),
                (1, &gtk_number_pages),
                (2, &gtk_current_page),
                (3, &gtk_percentage_progress),
                (
                    4,
                    &"Sent to the server, waiting to be converted.".to_string(),
                ),
            ];
            model.set(&model.append(), &values);
        }
        ConvertEvent::FileInfo {
            output_type: _,
            number_pages,
            file,
        } => {
            let gtk_number_pages = u32::from(*number_pages);
            model.foreach(|_tree_model, _tree_path, tree_iter| {
                let gtk_filename: String = model.get(tree_iter, 0).get::<String>().unwrap();
                if &gtk_filename == file {
                    model.set_value(tree_iter, 1, &gtk_number_pages.to_value());
                    model.set_value(tree_iter, 4, &"Ongoing".to_value());
                    return true;
                }
                false
            });
        }
        ConvertEvent::PageConverted { file, page } => {
            model.foreach(|_tree_model, _tree_path, tree_iter| {
                let gtk_filename: String = model.get(tree_iter, 0).get::<String>().unwrap();
                if &gtk_filename == file {
                    let total_page = model.get(tree_iter, 1).get::<u32>().unwrap();
                    debug!("PageConverted. {}: {}/{}", &file, &page, &total_page);
                    let gtk_page = model.get(tree_iter, 2).get::<u32>().unwrap() + 1;
                    #[allow(clippy::cast_possible_truncation)]
                    let percentage: f32 =
                        (f32::from(gtk_page as u16) + 1.0) * 100.0 / (f32::from(total_page as u16));
                    model.set_value(tree_iter, 2, &gtk_page.to_value());
                    model.set_value(tree_iter, 3, &percentage.to_value());
                    model.set_value(
                        tree_iter,
                        4,
                        &format!("Ongoing {}/{}", gtk_page, total_page).to_value(),
                    );
                    return true;
                }
                false
            });
        }
        ConvertEvent::FileConverted { file } => {
            model.foreach(|_tree_model, _tree_path, tree_iter| {
                let gtk_filename: String = model.get(tree_iter, 0).get::<String>().unwrap();
                if &gtk_filename == file {
                    let percentage: f32 = 100.0;
                    model.set_value(tree_iter, 3, &percentage.to_value());
                    model.set_value(tree_iter, 4, &"Done".to_value());
                    return true;
                }
                false
            });
        }
        ConvertEvent::Failure { file, message } => {
            model.foreach(|_tree_model, _tree_path, tree_iter| {
                let gtk_filename: String = model.get(tree_iter, 0).get::<String>().unwrap();
                if &gtk_filename == file {
                    model.set_value(tree_iter, 4, &format!("Failure: {}", message).to_value());
                    return true;
                }
                false
            });
        }
    }
    Continue(true)
}
