//! Route logical custom tabs through their single registered window.
use openidea_plugin::Error;
const CONFIG: &str = include_str!("ui_config.json");
pub(crate) fn reveal_view(view: &str) -> Result<(), Error> { ui_kit::web::reveal_view(view, CONFIG) }
pub(crate) fn refresh_view(view: &str) { ui_kit::web::refresh_view(view, CONFIG); }
pub(crate) fn post_view_message(view: &str, message: &str) -> Result<(), Error> { ui_kit::web::post_view_message(view, message, CONFIG) }
