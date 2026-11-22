//! Automatic sign-in to the engine's web page from the embedded web view.
//!
//! The page lives at `http://127.0.0.1:<web port>/admin/`; its sign-in is an API call, `POST /admin/api/login`
//! (the one route besides first-run `setup` that needs no session, see `config`). The view runs the script built
//! here once the page has loaded: it posts the stored administrator credentials from the page's own origin, so the
//! engine's `Host`/`Origin` check passes and the session cookie lands in the view's own store, then reloads. If the
//! post is not accepted it tries the usual form encoding, then fills the page's own sign-in form.
//!
//! **Unverified against the real engine**: the request body field names (`username`, `password`) and the form
//! fallback are the common shapes, not read from the engine's source. A failure leaves the sign-in page showing,
//! which is where the player would have been anyway.
//!
//! The script holds the password, so it is built on demand, handed to the view and never logged or stored. The
//! password is embedded only as a JSON string literal with `<`, `>`, `&` and the line separators escaped.

/// A JSON string literal safe to place in a script.
fn js_literal(text: &str) -> String {
    let json = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into());
    json.replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// The script for the page at `origin` (for example `http://127.0.0.1:8080`). It does nothing on any other origin,
/// so the password cannot be sent anywhere else.
pub fn script(origin: &str, username: &str, password: &str) -> String {
    format!(
        r#"(function(){{
var ORIGIN={origin},USER={user},PASS={pass};
if(location.origin!==ORIGIN)return;
var URL='/admin/api/login';
function done(r){{if(r&&r.ok)location.reload();else return false}}
function form(){{
var p=document.querySelector('input[type=password]');if(!p)return;
var f=p.form||document,u=f.querySelector('input[type=text],input[type=email],input:not([type])');
function set(el,v){{var s=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set;s.call(el,v);el.dispatchEvent(new Event('input',{{bubbles:true}}));el.dispatchEvent(new Event('change',{{bubbles:true}}))}}
if(u)set(u,USER);set(p,PASS);
var b=(p.form||document).querySelector('button[type=submit],input[type=submit],button');if(b)b.click()}}
fetch(URL,{{method:'POST',credentials:'same-origin',headers:{{'Content-Type':'application/json'}},body:JSON.stringify({{username:USER,password:PASS}})}})
.then(function(r){{if(done(r)===false)return fetch(URL,{{method:'POST',credentials:'same-origin',headers:{{'Content-Type':'application/x-www-form-urlencoded'}},body:'username='+encodeURIComponent(USER)+'&password='+encodeURIComponent(PASS)}}).then(function(r2){{if(done(r2)===false)form()}})}})
.catch(function(){{form()}});
}})();"#,
        origin = js_literal(origin),
        user = js_literal(username),
        pass = js_literal(password),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_password_is_only_a_json_literal() {
        let s = script(
            "http://127.0.0.1:8080",
            "skradle",
            "pa\"ss\\w</script>&\n'x",
        );
        assert!(
            s.contains(r#"PASS="pa\"ss\\w\u003c/script\u003e\u0026\n'x""#),
            "{s}"
        );
        assert!(!s.contains("</script>"));
    }

    #[test]
    fn the_script_is_bound_to_the_origin_and_the_login_route() {
        let s = script("http://127.0.0.1:8080", "skradle", "secret-password");
        assert!(s.contains(r#"ORIGIN="http://127.0.0.1:8080""#));
        assert!(s.contains("if(location.origin!==ORIGIN)return;"));
        assert!(s.contains("'/admin/api/login'"));
        assert!(s.contains(r#"USER="skradle""#));
    }

    #[test]
    fn line_separators_and_quotes_in_the_user_name_are_escaped() {
        let s = script("http://127.0.0.1:1", "a\u{2028}b\"", "x");
        assert!(s.contains(r#"USER="a\u2028b\"""#));
        assert!(!s.contains('\u{2028}'));
    }
}
