use notify_rust::Notification;
use thiserror::Error;

use crate::message::NotificationMessage;

pub trait Notifier {
    fn notify(&self, message: &NotificationMessage) -> Result<(), NotifyError>;
}

#[derive(Debug, Clone)]
pub struct DesktopNotifier {
    app_name: String,
}

#[derive(Debug, Error)]
pub enum NotifyError {
    #[error("desktop notifications are disabled in config")]
    Disabled,
    #[error("failed to show desktop notification: {0}")]
    Desktop(#[from] notify_rust::error::Error),
}

impl DesktopNotifier {
    pub fn new(app_name: impl Into<String>) -> Self {
        Self {
            app_name: app_name.into(),
        }
    }
}

impl Notifier for DesktopNotifier {
    fn notify(&self, message: &NotificationMessage) -> Result<(), NotifyError> {
        Notification::new()
            .appname(&self.app_name)
            .summary(&message.title)
            .body(&message.body)
            .show()?;

        Ok(())
    }
}

pub fn notify_if_enabled<N: Notifier>(
    notifier: &N,
    desktop_enabled: bool,
    message: &NotificationMessage,
) -> Result<(), NotifyError> {
    if !desktop_enabled {
        return Err(NotifyError::Disabled);
    }

    notifier.notify(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    #[derive(Debug, Clone)]
    struct FakeNotifier {
        messages: Rc<RefCell<Vec<NotificationMessage>>>,
    }

    impl FakeNotifier {
        fn new() -> Self {
            Self {
                messages: Rc::new(RefCell::new(Vec::new())),
            }
        }
    }

    impl Notifier for FakeNotifier {
        fn notify(&self, message: &NotificationMessage) -> Result<(), NotifyError> {
            self.messages.borrow_mut().push(message.clone());
            Ok(())
        }
    }

    #[test]
    fn sends_when_enabled() {
        let notifier = FakeNotifier::new();
        let message = NotificationMessage {
            title: "地震情報".to_owned(),
            body: "最大震度: 4".to_owned(),
        };

        notify_if_enabled(&notifier, true, &message).expect("notification should be sent");

        assert_eq!(notifier.messages.borrow().as_slice(), &[message]);
    }

    #[test]
    fn rejects_when_disabled() {
        let notifier = FakeNotifier::new();
        let message = NotificationMessage {
            title: "地震情報".to_owned(),
            body: "最大震度: 4".to_owned(),
        };

        let error = notify_if_enabled(&notifier, false, &message).expect_err("should be disabled");

        assert!(matches!(error, NotifyError::Disabled));
        assert!(notifier.messages.borrow().is_empty());
    }
}
