//! `ServerHost` (QML singleton): state and address of the player's own server, from the server-host module
//! (a stub for now, see `crate::server_host`).

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// "stopped", "starting", "ready" or "failed".
        #[qproperty(QString, state, READ, WRITE, NOTIFY)]
        /// The management UI's address when ready.
        #[qproperty(QString, url, READ, WRITE, NOTIFY)]
        #[qproperty(QString, message, READ, WRITE, NOTIFY)]
        type ServerHost = super::ServerHostRust;

        /// Asks the server-host module for its status again.
        #[qinvokable]
        fn refresh(self: Pin<&mut ServerHost>);
    }

    impl cxx_qt::Initialize for ServerHost {}
}

use core::pin::Pin;

use cxx_qt::Initialize;
use cxx_qt_lib::QString;

use crate::server_host::{ServerHost as _, StubHost};

#[derive(Default)]
pub struct ServerHostRust {
    state: QString,
    url: QString,
    message: QString,
}

impl qobject::ServerHost {
    pub fn refresh(mut self: Pin<&mut Self>) {
        // TODO(serverhost): the real host.
        let status = StubHost.status();
        self.as_mut()
            .set_state(QString::from(status.state.as_str()));
        self.as_mut()
            .set_url(QString::from(status.url.unwrap_or_default().as_str()));
        self.set_message(QString::from(status.message.as_str()));
    }
}

impl Initialize for qobject::ServerHost {
    fn initialize(self: Pin<&mut Self>) {
        self.refresh();
    }
}
