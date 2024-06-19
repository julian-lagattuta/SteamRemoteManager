use serde::{Serialize,Deserialize};
#[derive(Serialize,Deserialize,Debug)]
pub enum ClientMessage{
    Join,
    Ok,
    Error(String),
    LoginSuccess,
    Account{
        username: String,
        password: String
    }
}
#[derive(Serialize,Deserialize,Debug)]
pub enum ServerMessage{
    Hello,
    SetAccount{
        username: String,
        password: String
    },
    GetAccount,
    Login,
    Logout

}