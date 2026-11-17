// A Rust object exposed to QML: the shape the launcher core would take (state + invokable methods).
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(i32, scale)]
        #[qproperty(QString, status)]
        type Backend = super::BackendRust;

        #[qinvokable]
        fn save(self: Pin<&mut Backend>);
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;

pub struct BackendRust {
    scale: i32,
    status: QString,
}

impl Default for BackendRust {
    fn default() -> Self {
        Self { scale: 100, status: QString::from("not saved") }
    }
}

impl qobject::Backend {
    pub fn save(mut self: Pin<&mut Self>) {
        let msg = format!("saved: render scale {}% (computed in Rust)", self.scale());
        self.as_mut().set_status(QString::from(&msg));
    }
}
