#[repr(usize)]
#[derive(Hash, PartialEq, Eq, Ord, PartialOrd, Debug, Copy, Clone)]
pub enum S1Event {
    AccountNameRequest = 1,
    AccountNameResponse = 2,
    SessionTicketRequest = 3,
    SessionTicketResponse = 4,
    ServerListRequest = 5,
    ServerListResponse = 6,
    EnterLobbyOrWorld = 7,
    CreateRoomRequest = 8,
    CreateRoomResponse = 9,
    JoinRoomRequest = 10,
    JoinRoomResponse = 11,
    LeaveRoomRequest = 12,
    LeaveRoomResponse = 13,
    SetVolumeCommand = 19,
    SetMicrophoneCommand = 20,
    SilenceUserCommand = 21,
    OpenWebsiteCommand = 25,
    WebUrlRequest = 26,
    WebUrlResponse = 27,
    GameStart = 1000,
    EnteredIntoCinematic = 1001,
    EnteredServerList = 1002,
    EnteringLobby = 1003,
    EnteredLobby = 1004,
    EnteringCharacterCreation = 1005,
    LeftLobby = 1006,
    DeletedCharacter = 1007,
    CanceledCharacterCreation = 1008,
    EnteredCharacterCreation = 1009,
    CreatedCharacter = 1010,
    EnteredWorld = 1011,
    FinishedLoadingScreen = 1012,
    LeftWorld = 1013,
    MountedPegasus = 1014,
    DismountedPegasus = 1015,
    ChangedChannel = 1016,
    GameExit = 1020,
    GameCrash = 1021,
    AntiCheatStarting = 1022,
    AntiCheatStarted = 1023,
    AntiCheatError = 1024,
    OpenSupportWebsiteCommand = 1025,
    Other(usize),
}

impl From<usize> for S1Event {
    fn from(value: usize) -> Self {
        match value {
            1 => S1Event::AccountNameRequest,
            2 => S1Event::AccountNameResponse,
            3 => S1Event::SessionTicketRequest,
            4 => S1Event::SessionTicketResponse,
            5 => S1Event::ServerListRequest,
            6 => S1Event::ServerListResponse,
            7 => S1Event::EnterLobbyOrWorld,
            8 => S1Event::CreateRoomRequest,
            9 => S1Event::CreateRoomResponse,
            10 => S1Event::JoinRoomRequest,
            11 => S1Event::JoinRoomResponse,
            12 => S1Event::LeaveRoomRequest,
            13 => S1Event::LeaveRoomResponse,
            19 => S1Event::SetVolumeCommand,
            20 => S1Event::SetMicrophoneCommand,
            21 => S1Event::SilenceUserCommand,
            25 => S1Event::OpenWebsiteCommand,
            26 => S1Event::WebUrlRequest,
            27 => S1Event::WebUrlResponse,
            1000 => S1Event::GameStart,
            1001 => S1Event::EnteredIntoCinematic,
            1002 => S1Event::EnteredServerList,
            1003 => S1Event::EnteringLobby,
            1004 => S1Event::EnteredLobby,
            1005 => S1Event::EnteringCharacterCreation,
            1006 => S1Event::LeftLobby,
            1007 => S1Event::DeletedCharacter,
            1008 => S1Event::CanceledCharacterCreation,
            1009 => S1Event::EnteredCharacterCreation,
            1010 => S1Event::CreatedCharacter,
            1011 => S1Event::EnteredWorld,
            1012 => S1Event::FinishedLoadingScreen,
            1013 => S1Event::LeftWorld,
            1014 => S1Event::MountedPegasus,
            1015 => S1Event::DismountedPegasus,
            1016 => S1Event::ChangedChannel,
            1020 => S1Event::GameExit,
            1021 => S1Event::GameCrash,
            1022 => S1Event::AntiCheatStarting,
            1023 => S1Event::AntiCheatStarted,
            1024 => S1Event::AntiCheatError,
            1025 => S1Event::OpenSupportWebsiteCommand,
            other => S1Event::Other(other),
        }
    }
}

impl S1Event {
    /// Returns true if this event should trigger stopping the mirror client
    pub fn should_stop_mirror_client(&self) -> bool {
        matches!(self,
            S1Event::LeftLobby |
            S1Event::GameExit |
            S1Event::GameCrash
        )
    }
    
    /// Returns true if this event should trigger starting the mirror client
    pub fn should_start_mirror_client(&self) -> bool {
        matches!(self,
            S1Event::EnteredLobby
        )
    }
}
