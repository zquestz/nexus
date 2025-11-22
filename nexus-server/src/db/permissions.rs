//! Permission system for user authorization

use std::collections::HashSet;

/// Permission types for user actions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    /// Permission to list connected users
    ListUsers,
    /// Permission to send chat messages
    SendChat,
    /// Permission to receive chat messages
    ReceiveChat,
}

impl Permission {
    /// Convert permission to string for database storage
    pub fn as_str(&self) -> &'static str {
        match self {
            Permission::ListUsers => "list_users",
            Permission::SendChat => "send_chat",
            Permission::ReceiveChat => "receive_chat",
        }
    }

    /// Parse permission from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "list_users" => Some(Permission::ListUsers),
            "send_chat" => Some(Permission::SendChat),
            "receive_chat" => Some(Permission::ReceiveChat),
            _ => None,
        }
    }

    /// Get all available permissions
    pub fn all() -> Vec<Permission> {
        vec![
            Permission::ListUsers,
            Permission::SendChat,
            Permission::ReceiveChat,
        ]
    }
}

/// Set of permissions for a user
#[derive(Debug, Clone)]
pub struct Permissions {
    permissions: HashSet<Permission>,
}

impl Permissions {
    /// Create a new empty permission set
    pub fn new() -> Self {
        Self {
            permissions: HashSet::new(),
        }
    }

    /// Create a permission set with all permissions
    pub fn all() -> Self {
        Self {
            permissions: Permission::all().into_iter().collect(),
        }
    }

    /// Create permissions from a list
    pub fn from_vec(perms: Vec<Permission>) -> Self {
        Self {
            permissions: perms.into_iter().collect(),
        }
    }

    /// Check if user has a specific permission
    pub fn has(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission)
    }

    /// Add a permission
    pub fn add(&mut self, permission: Permission) {
        self.permissions.insert(permission);
    }

    /// Remove a permission
    pub fn remove(&mut self, permission: Permission) {
        self.permissions.remove(&permission);
    }

    /// Get all permissions as a vec
    pub fn to_vec(&self) -> Vec<Permission> {
        self.permissions.iter().copied().collect()
    }
}

impl Default for Permissions {
    fn default() -> Self {
        Self::new()
    }
}