//! Writable app-private files on Android; desktop keeps the existing data folder.
use std::path::PathBuf;
pub fn game_file(name: &str) -> Result<PathBuf, String> {
    if name.contains('/') || name.contains('\\') || name.is_empty() {
        return Err("Invalid app file name".into());
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(PathBuf::from("data").join(name))
    }
    #[cfg(target_os = "android")]
    unsafe {
        use macroquad::miniquad::native::android::{ACTIVITY, attach_jni_env};
        let env = attach_jni_env();
        let activity = ACTIVITY;
        if env.is_null() || activity.is_null() {
            return Err("Android activity unavailable".into());
        }
        let table = &**env;
        let class = table
            .GetObjectClass
            .ok_or("JNI GetObjectClass unavailable")?(env, activity);
        if class.is_null() {
            return Err("Android activity class unavailable".into());
        }
        let get = table.GetMethodID.ok_or("JNI GetMethodID unavailable")?(
            env,
            class,
            c"getFilesDir".as_ptr(),
            c"()Ljava/io/File;".as_ptr(),
        );
        if get.is_null() {
            table.ExceptionClear.unwrap()(env);
            return Err("Android files directory method unavailable".into());
        }
        let file = table.CallObjectMethod.unwrap()(env, activity, get);
        if file.is_null() {
            return Err("Android files directory unavailable".into());
        }
        let file_class = table.GetObjectClass.unwrap()(env, file);
        let absolute = table.GetMethodID.unwrap()(
            env,
            file_class,
            c"getAbsolutePath".as_ptr(),
            c"()Ljava/lang/String;".as_ptr(),
        );
        if absolute.is_null() {
            table.ExceptionClear.unwrap()(env);
            return Err("Android path method unavailable".into());
        }
        let path = table.CallObjectMethod.unwrap()(env, file, absolute);
        if path.is_null() {
            return Err("Android path unavailable".into());
        }
        let chars = table.GetStringUTFChars.unwrap()(env, path, std::ptr::null_mut());
        if chars.is_null() {
            return Err("Android UTF path unavailable".into());
        }
        let result = std::ffi::CStr::from_ptr(chars)
            .to_str()
            .map(|s| PathBuf::from(s).join(name))
            .map_err(|e| e.to_string());
        table.ReleaseStringUTFChars.unwrap()(env, path, chars);
        for obj in [path, file_class, file, class] {
            table.DeleteLocalRef.unwrap()(env, obj);
        }
        result
    }
}
