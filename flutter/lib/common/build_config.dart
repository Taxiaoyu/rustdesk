const bool kIsAdminEdition =
    bool.fromEnvironment('RUSTDESK_ADMIN_EDITION', defaultValue: false);

const String kBundledRemoteVaultUrl = 'http://rd.chuan-chuan.com:13002';

const String kRemoteVaultUrl =
    String.fromEnvironment('RUSTDESK_REMOTE_VAULT_URL',
        defaultValue: kBundledRemoteVaultUrl);
