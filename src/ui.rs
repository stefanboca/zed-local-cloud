use topcoat::{Result, context::Cx, view::view};

pub async fn sign_in(cx: &Cx, dev: bool) -> Result {
    let __cx = cx;
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>"Sign in to Zed"</title>
                <style>
                    r#":root { color-scheme: light dark; font-family: system-ui, sans-serif; }
                    body { min-height: 100vh; margin: 0; display: grid; place-items: center; background: #18181b; color: #fafafa; }
                    main { width: min(28rem, calc(100% - 3rem)); padding: 2rem; border: 1px solid #3f3f46; border-radius: 0.75rem; background: #27272a; }
                    h1 { margin-top: 0; font-size: 1.5rem; }
                    p { color: #d4d4d8; line-height: 1.5; }
                    button { width: 100%; margin-top: 1rem; padding: 0.8rem 1rem; border: 0; border-radius: 0.5rem; background: #7c3aed; color: white; font: inherit; font-weight: 650; cursor: pointer; }
                    button:hover { background: #6d28d9; }"#
                </style>
                if dev {
                    topcoat::dev::script()
                }
            </head>
            <body>
                <main>
                    <h1>"Sign in to Zed"</h1>
                    <form method="post" action="/auth/oidc/start">
                        <button type="submit">"Continue with OpenID Connect"</button>
                    </form>
                </main>
            </body>
        </html>
    }
}

pub async fn sign_in_succeeded(cx: &Cx, dev: bool) -> Result {
    let __cx = cx;
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>"Zed sign-in complete"</title>
                if dev {
                    topcoat::dev::script()
                }
            </head>
            <body>
                <main>
                    <h1>"Sign-in complete"</h1>
                    <p>"You can close this page and return to Zed."</p>
                </main>
            </body>
        </html>
    }
}
