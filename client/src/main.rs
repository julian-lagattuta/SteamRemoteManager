use std::ffi::{CString, OsStr, OsString};
use std::io::Read;
use std::ops::Add;
use rsautogui::{mouse};
use std::ptr::{null, null_mut};
use winapi::um;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::ffi::OsStringExt;
use std::{slice, thread};
use std::fs::{File, OpenOptions};
use rsautogui::mouse::{Button, click, move_to};
use winapi::um::winuser;
use winapi::um::winuser::{DestroyWindow, EnumWindows, GetWindowModuleFileNameW, GetWindowRect, GetWindowThreadProcessId, PostMessageW, WM_CLOSE};
use std::thread::sleep;
use std::time::Duration;
use winapi::shared::minwindef::{DWORD, HKEY, LPARAM, PHKEY, TRUE, UINT};
use winapi::shared::windef::HWND;
use winapi::um::libloaderapi::GetModuleFileNameW;
use winapi::um::processthreadsapi;
use winapi::um::processthreadsapi::{GetProcessId, OpenProcess, TerminateProcess};
use std::str::FromStr;
use winapi::ctypes::c_void;
use winapi::um::winreg::HKEY_CURRENT_USER;
use windows_sys::w;
use anyhow::{anyhow, Result, Context};
use std::os::windows::fs::FileExt;
use chrono;
use std::net::{ UdpSocket};
use serde::{Serialize,Deserialize};
use std::result;
use shared::{ClientMessage,ServerMessage};
use postcard;

struct Session {
    username: String,
    password:String,
    server: String,
    port:String
}

unsafe extern "system" fn callback(handle: HWND, param: LPARAM)->i32{
    let mut pid : DWORD =0;

    unsafe{
        GetWindowThreadProcessId(handle,&mut pid);
    };

    if pid ==param as u32{
        let proc = OpenProcess(1,0,pid);

        TerminateProcess(proc as *mut c_void,0 ) ;
    }
    return 1;
}
impl Session{
    fn get_steam_pid()->Result<u32>{
        let output = std::process::Command::new("tasklist").output().unwrap().stdout;
        let output = String::from_utf8_lossy(&output);
        let entries =output.split("\n");

        let mut pid = String::new();
        for entry in entries{
            if entry.starts_with("steam.exe"){

                for x in entry.chars(){

                    if x.is_ascii_digit(){
                        pid.push(x);
                    }else if(pid.len()>0){
                        return Ok(u32::from_str(pid.as_str()).unwrap())
                    }
                }
            }
        }
        return Err(anyhow!("failed to find pid for steam"))
    }
    fn get_steam_exe_path()-> Result<String>{
        let mut size:DWORD = 512;
        let mut buffer: Vec<u8> = Vec::with_capacity(size as usize);
        unsafe{
            let path =w!(r"SOFTWARE\Valve\Steam");
            let err= winapi::um::winreg::RegGetValueW(HKEY_CURRENT_USER,path,w!("SteamExe"),0xffff,null_mut(), buffer.as_mut_ptr() as *mut c_void,&mut size);
            if err !=0 {
                return Err(anyhow!("failed to find steam registry with error_code {}",err));
            }
            buffer.set_len(size as usize);
        };


        let words = unsafe{
            slice::from_raw_parts(buffer.as_ptr() as *const u16,buffer.len()/2)
        };
        let mut s = String::from_utf16_lossy(words);
        while s.ends_with("\u{0}"){
            s.pop();
        }
        return Ok(s)
    }
    fn get_steam_path()->Result<String>{
        let s = Self::get_steam_exe_path()?;
        let  mut chars = s.chars();
        chars.nth_back("steam.exe".len()-1);
        Ok(chars.collect())

    }
    fn is_steam_open()-> bool{
        let output = std::process::Command::new("tasklist").output().unwrap().stdout;
        let output = String::from_utf8_lossy(&output);
        let entries =output.split("\n");

        let mut pid = String::new();
        for entry in entries{
            if entry.starts_with("steam.exe"){
                return true;
            }

        }
        return false;
    }
    unsafe fn kill_steam(){
        EnumWindows(Some(callback),Session::get_steam_pid().unwrap() as isize);
    }
    fn close_steam(wait_seconds: f64) ->Result<()>{

        if !Self::is_steam_open(){
            return Ok(())
        }
        let s= Self::get_steam_exe_path()?;

        std::process::Command::new(s).arg("+quit").output().unwrap();
        let mut total_wait = 0.;
        while(Self::is_steam_open() && total_wait<wait_seconds){
            sleep(Duration::from_millis(500));
            total_wait+=0.5;
        }


        if total_wait>=wait_seconds{

            unsafe{
                Self::kill_steam();
            }
            sleep(Duration::from_millis(1000));
            if Self::is_steam_open(){
                return Err(anyhow!("failed to close Steam after {} seconds!",wait_seconds))
            }
        }
        Ok(())
    }
    fn logout(forget_password:bool)->Result<()>{
        Self::close_steam(10.)?;

        let file_name = Self::get_steam_path()? + "config/loginusers.vdf";
        let mut file = OpenOptions::new().read(true).write(true).open(file_name).context("failed to open loginusers.vdf")?;
        let mut buffer: Vec<u8> = Vec::new();
        file.read_to_end(&mut buffer).context("failed to read loginusers.vdf")?;
        let buffer = String::from_utf8(buffer).context("failed to read utf8 loginusers.vdf")?;


        let zero = "0";
        for (pos,x_) in buffer.match_indices("AllowAutoLogin"){
            let mut current_pos = pos;
            while current_pos<buffer.len() && !buffer.chars().nth(current_pos).unwrap().is_ascii_digit(){
                current_pos+=1;
            }
            file.seek_write(zero.as_bytes(),current_pos.try_into().unwrap()).context("failed to write into file while logging out")?;
        }

        if forget_password{
            for (pos,x_) in buffer.match_indices("RememberPassword"){
                let mut current_pos = pos;
                while current_pos<buffer.len() && !buffer.chars().nth(current_pos).unwrap().is_ascii_digit(){
                    current_pos+=1;
                }
                file.seek_write(zero.as_bytes(),current_pos.try_into().unwrap()).context("failed to write into file with forgetting password")?;
            }

        }
        Ok(())
    }
    fn login_manually(&self){

        let name :Vec<u16> =OsString::from("Sign in to Steam").as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let handle = unsafe{
            winuser::FindWindowW(null(),name.as_ptr())
        };

        unsafe{
            winuser::SetForegroundWindow(handle)
        };
        let mut rect: winapi::shared::windef::RECT= unsafe{std::mem::zeroed()};
        unsafe{
            GetWindowRect(handle,&mut rect)
        };
        sleep(Duration::from_millis(1500));
        move_to(rect.right as u16 -100,rect.top as u16+242);
        click(Button::Left);

        sleep(Duration::from_millis(500));
        move_to(rect.left as u16 +98,rect.top as u16+137);
        click(Button::Left);
        rsautogui::keyboard::typewrite(&self.username);

        sleep(Duration::from_millis(500));
        move_to(rect.left as u16 +178,rect.top as u16+216);
        click(Button::Left);
        rsautogui::keyboard::typewrite(&self.password);

        sleep(Duration::from_millis(500));
        move_to(rect.left as u16 +173, rect.top as u16 +309);
        click(Button::Left);
    }
    fn open_steam()->Result<()>{
        let path = Self::get_steam_exe_path()?;
        std::process::Command::new(path).spawn().unwrap();
        Ok(())
    }

    fn verify_logged_in(&self,acceptable_difference:i64)->Result<(bool)>{
        let file_name = Self::get_steam_path()? + "config/loginusers.vdf";
        let mut file = OpenOptions::new().read(true).write(false).open(file_name).context("failed to open loginusers.vdf")?;
        let mut buffer: Vec<u8> = Vec::new();
        file.read_to_end(&mut buffer).context("failed to read loginusers.vdf")?;
        let buffer = String::from_utf8(buffer).context("failed to read utf8 loginusers.vdf")?;


        let pos =match buffer.find(&self.username) {
            None => {
                return Ok(false)
            },
            Some(p) => p
        };
        let mut current_pos = buffer[pos..].find("Timestamp").context("could not find timestamp")?;
        let mut s= String::new();
        while current_pos<buffer.len(){
            if buffer.chars().nth(current_pos).unwrap().is_ascii_digit(){
                s.push(buffer.chars().nth(current_pos).unwrap());
            }else if(s.len()>0){
                break;
            }
            current_pos+=1;

        }
        let real_time = chrono::Utc::now().timestamp();
        let login_time = i64::from_str(s.as_str()).unwrap();
        if login_time-real_time<=acceptable_difference{
            return Ok(true)
        }

        Ok(false)
    }
    fn login(&self,seconds:i32)->Result<()>{
        Self::close_steam(seconds as f64)?;
        Self::logout(false)?;
        println!("loggout");
        Self::open_steam()?;

        sleep(Duration::from_secs(10));
        self.login_manually();
        Ok(())
    }
    fn connect(&self,ping_wait:Duration)->UdpSocket{
        println!("connecting...");
        let socket = UdpSocket::bind(format!("127.0.0.1:{}",self.port)).unwrap();
        socket.connect(self.server.as_str()).unwrap();
        let join = ClientMessage::Join;
        let join = postcard::to_allocvec(&join).unwrap();
        socket.set_read_timeout(Some(ping_wait)).unwrap();
        let mut response: Vec<u8> = vec![0;1024];
        loop{
            socket.send(&join).unwrap();
            println!("ping sent");
            match socket.recv(response.as_mut_slice()) {
                Ok(_)=>{
                    socket.set_read_timeout(None).unwrap();
                    return socket},
                Err(e) => {
                    sleep(ping_wait);
                }
            };


        }


    }
    fn send_error(&self,socket:&UdpSocket,err : &(dyn std::error::Error)){
        let message = format!("{}",err);

        socket.send(message.as_bytes()).unwrap();
    }
    fn run(&self){
        let socket = self.connect(Duration::from_secs(10));
        println!("successfully connected");
        loop{
            let mut buffer = Vec::with_capacity(1024);
            socket.recv(&mut buffer).unwrap();
            let message:ServerMessage  =match postcard::from_bytes(buffer.as_slice()) {
                Err(e)=> {self.send_error(&socket,&e); continue},
                Ok(n)=>{n}
            };
            println!("{:?}",message);

        }
    }

}


fn main() {
    // Session::close_steam(10.).unwrap();
    // Session::logout(false).unwrap();
    let s=  Session{
        username:"freekidsc".to_string(),
        password:"helloWORLD1".to_string(),
        server:"127.0.0.1:8080".to_string(),
        port:"1234".to_string()
    };
    s.run();
    // s.login(10).unwrap();

    let socket = UdpSocket::bind("127.0.0.1:8080").unwrap();
    socket.connect("127.0.0.1:12345").unwrap();
    socket.set_read_timeout(Some(Duration::from_secs(500))).unwrap();
    let mut buffer = [0;256];

    socket.recv(&mut buffer).unwrap();
    // Session::close_steam(10.).unwrap();
    // Session::logout(false).unwrap();

    // println!("Hello, world! {} {}",handle as i32,k as i32);
}