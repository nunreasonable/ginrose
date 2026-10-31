//! What Ginrose Studio hands Roblox Studio: the links the browser opens, place
//! files, and the user's own arguments.

/// What `ginrose-studio launch` was given.
#[derive(Debug, PartialEq, Eq)]
pub enum Target<'a> {
    /// Nothing: open Studio, or bring the running one forward.
    None,
    /// A browser login coming back (`roblox-studio-auth:`), for the Studio
    /// that asked for it.
    Login(&'a str),
    /// The website's "Edit in Studio" (`roblox-studio:`).
    Edit(&'a str),
    /// A place file or anything else, for Studio to open as a document.
    Other(&'a str),
}

fn scheme(text: &str) -> Option<&str> {
    let (scheme, _) = text.split_once(':')?;
    (scheme.len() >= 2
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')))
    .then_some(scheme)
}

pub fn target(uri: Option<&str>) -> Target<'_> {
    let Some(uri) = uri.map(str::trim).filter(|u| !u.is_empty()) else {
        return Target::None;
    };
    match scheme(uri).map(str::to_ascii_lowercase).as_deref() {
        Some("roblox-studio-auth") => Target::Login(uri),
        Some("roblox-studio") => Target::Edit(uri),
        _ => Target::Other(uri),
    }
}

/// What follows `rosemary run`: whether to start another instance, then the
/// app and Studio's arguments.
///
/// A login goes to the Studio already running, as a URL; Rosemary hands it
/// over, the way a Mac sends it as an Apple Event. "Edit in Studio" is what
/// the protocol helper inside Studio does on a Mac: start another Studio
/// process with `-protocolString <link> -protocolHandlerLaunch`, whatever
/// runs already. A place file is opened like a document.
pub fn arguments(target: &Target, extra: &[String]) -> (bool, Vec<String>) {
    match target {
        Target::None => (false, extra.to_vec()),
        Target::Login(uri) => (false, vec![uri.to_string()]),
        Target::Edit(uri) => {
            let mut args = extra.to_vec();
            args.extend([
                "-protocolString".to_string(),
                uri.to_string(),
                "-protocolHandlerLaunch".to_string(),
            ]);
            (true, args)
        }
        Target::Other(path) => {
            let mut args = extra.to_vec();
            let path = std::path::Path::new(path);
            let absolute = match path.is_relative() {
                true => std::env::current_dir()
                    .map(|dir| dir.join(path))
                    .unwrap_or_else(|_| path.to_path_buf()),
                false => path.to_path_buf(),
            };
            args.push(absolute.to_string_lossy().into_owned());
            (false, args)
        }
    }
}

/// Split a line of arguments the way a shell would, for the extra launch
/// arguments: spaces separate, quotes group, a backslash escapes.
pub fn split_args(line: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') | (None, '\\') => match chars.next() {
                Some(next) => current.push(next),
                None => return Err("the line ends with a lone backslash".into()),
            },
            (Some(_), c) => current.push(c),
            (None, '"' | '\'') => quote = Some(c),
            (None, c) if c.is_whitespace() => {
                if started || !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
                started = false;
                continue;
            }
            (None, c) => current.push(c),
        }
        started = true;
    }
    if quote.is_some() {
        return Err("a quote is not closed".into());
    }
    if started || !current.is_empty() {
        args.push(current);
    }
    Ok(args)
}

/// The arguments back as one line, quoted where needed.
pub fn join_args(args: &[String]) -> String {
    args.iter()
        .map(|arg| {
            if !arg.is_empty()
                && arg
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_=./:,+@%".contains(c))
            {
                arg.clone()
            } else {
                format!("'{}'", arg.replace('\'', r"'\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOGIN: &str = "roblox-studio-auth:/?code=abc&state=xyz";
    const EDIT: &str = "roblox-studio:1+launchmode:edit+task:EditPlace+placeId:1818+universeId:9";

    #[test]
    fn links_are_told_apart() {
        assert_eq!(target(None), Target::None);
        assert_eq!(target(Some("  ")), Target::None);
        assert_eq!(target(Some(LOGIN)), Target::Login(LOGIN));
        assert_eq!(
            target(Some("ROBLOX-STUDIO-AUTH:/x")),
            Target::Login("ROBLOX-STUDIO-AUTH:/x")
        );
        assert_eq!(target(Some(EDIT)), Target::Edit(EDIT));
        assert_eq!(
            target(Some("/home/me/Game.rbxl")),
            Target::Other("/home/me/Game.rbxl")
        );
        // A Mac drive letter is no scheme.
        assert_eq!(target(Some("C:/x.rbxl")), Target::Other("C:/x.rbxl"));
    }

    #[test]
    fn a_login_goes_to_the_running_studio_alone() {
        let extra = vec!["-flag".to_string()];
        assert_eq!(
            arguments(&target(Some(LOGIN)), &extra),
            (false, vec![LOGIN.to_string()])
        );
    }

    #[test]
    fn edit_in_studio_starts_another_studio_like_the_mac_helper() {
        let extra = vec!["-flag".to_string()];
        let (new_instance, args) = arguments(&target(Some(EDIT)), &extra);
        assert!(new_instance);
        assert_eq!(
            args,
            ["-flag", "-protocolString", EDIT, "-protocolHandlerLaunch"]
        );
    }

    #[test]
    fn plain_launches_and_files_keep_the_extra_arguments() {
        let extra = vec!["-a".to_string(), "b c".to_string()];
        assert_eq!(arguments(&Target::None, &extra), (false, extra.clone()));
        let (new_instance, args) = arguments(&Target::Other("/tmp/Place.rbxl"), &extra);
        assert!(!new_instance);
        assert_eq!(args, ["-a", "b c", "/tmp/Place.rbxl"]);
        let (_, args) = arguments(&Target::Other("Place.rbxl"), &[]);
        assert!(std::path::Path::new(&args[0]).is_absolute());
    }

    #[test]
    fn arguments_split_like_a_shell() {
        assert_eq!(split_args("").unwrap(), Vec::<String>::new());
        assert_eq!(
            split_args(r#"-a  "b c" 'd "e"' f\ g """#).unwrap(),
            ["-a", "b c", r#"d "e""#, "f g", ""]
        );
        assert!(split_args("'open").is_err());
        let args: Vec<String> = ["-x", "two words", "it's", ""]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(split_args(&join_args(&args)).unwrap(), args);
    }
}
