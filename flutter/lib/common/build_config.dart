const bool kIsAdminEdition =
    bool.fromEnvironment('RUSTDESK_ADMIN_EDITION', defaultValue: false);

const String kRemoteVaultUrl =
    String.fromEnvironment('RUSTDESK_REMOTE_VAULT_URL', defaultValue: '');
