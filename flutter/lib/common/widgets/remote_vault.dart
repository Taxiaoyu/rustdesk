import 'dart:convert';
import 'package:flutter/material.dart';
import 'package:flutter_hbb/common.dart';
import 'package:flutter_hbb/models/peer_model.dart';
import 'package:http/http.dart' as http;

/// RemoteVault 服务器地址（部署后替换为实际公网 IP）
const kRemoteVaultUrl = 'http://118.31.42.84:13002';

/// 从 RemoteVault API 获取远程码列表
Future<List<RemoteVaultEntry>> fetchRemoteVaultEntries() async {
  try {
    final uri = Uri.parse('$kRemoteVaultUrl/api/codes?type=RustDesk');
    final response = await http.get(uri).timeout(const Duration(seconds: 5));

    if (response.statusCode == 200) {
      final list = jsonDecode(response.body) as List<dynamic>;
      return list
          .map((e) => RemoteVaultEntry.fromJson(e as Map<String, dynamic>))
          .toList();
    }
  } catch (_) {
    // 网络错误或超时，返回空列表
  }
  return [];
}

/// RemoteVault 远程码条目
class RemoteVaultEntry {
  final int id;
  final String code;
  final String? password;
  final String projectName;
  final String? notes;

  RemoteVaultEntry({
    required this.id,
    required this.code,
    this.password,
    required this.projectName,
    this.notes,
  });

  factory RemoteVaultEntry.fromJson(Map<String, dynamic> json) {
    return RemoteVaultEntry(
      id: json['id'] as int,
      code: (json['code'] as String).replaceAll(' ', ''),
      password: json['password'] as String?,
      projectName: json['projectName'] as String? ?? '',
      notes: json['notes'] as String?,
    );
  }
}

/// 弹出 RemoteVault 远程码选择对话框
Future<void> showRemoteVaultDialog(BuildContext context) async {
  showDialog(
    context: context,
    builder: (ctx) => _RemoteVaultDialog(),
  );
}

class _RemoteVaultDialog extends StatefulWidget {
  @override
  State<_RemoteVaultDialog> createState() => _RemoteVaultDialogState();
}

class _RemoteVaultDialogState extends State<_RemoteVaultDialog> {
  List<RemoteVaultEntry> _entries = [];
  bool _loading = true;
  String? _error;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    final entries = await fetchRemoteVaultEntries();
    if (!mounted) return;
    setState(() {
      _entries = entries;
      _loading = false;
      _error = entries.isEmpty ? '没有找到 RustDesk 远程码（请先在 RemoteVault 中添加）' : null;
    });
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: Row(
        children: [
          const Text('📡 RemoteVault', style: TextStyle(fontSize: 18)),
          const Spacer(),
          IconButton(
            icon: const Icon(Icons.refresh, size: 20),
            onPressed: () {
              setState(() {
                _loading = true;
                _error = null;
              });
              _load();
            },
          ),
          IconButton(
            icon: const Icon(Icons.close, size: 20),
            onPressed: () => Navigator.of(context).pop(),
          ),
        ],
      ),
      content: SizedBox(
        width: 400,
        child: _buildContent(),
      ),
    );
  }

  Widget _buildContent() {
    if (_loading) {
      return const SizedBox(
        height: 100,
        child: Center(child: CircularProgressIndicator(strokeWidth: 2)),
      );
    }

    if (_error != null && _entries.isEmpty) {
      return SizedBox(
        height: 80,
        child: Center(
          child: Text(_error!, style: const TextStyle(color: Colors.grey)),
        ),
      );
    }

    return SizedBox(
      height: (_entries.length * 56.0).clamp(0, 350).toDouble(),
      child: ListView.builder(
        shrinkWrap: true,
        itemCount: _entries.length,
        itemBuilder: (context, index) {
          final entry = _entries[index];
          return ListTile(
            dense: true,
            leading: Container(
              width: 32,
              height: 32,
              decoration: BoxDecoration(
                color: Colors.orange.shade50,
                borderRadius: BorderRadius.circular(8),
              ),
              child: const Center(child: Text('🦀', fontSize: 16)),
            ),
            title: Text(
              entry.projectName,
              style: const TextStyle(fontWeight: FontWeight.w600, fontSize: 14),
            ),
            subtitle: Text(
              entry.code,
              style: TextStyle(
                fontFamily: 'monospace',
                fontSize: 13,
                color: Colors.grey.shade700,
              ),
            ),
            trailing: entry.password != null && entry.password!.isNotEmpty
                ? const Icon(Icons.vpn_key, size: 16, color: Colors.orange)
                : null,
            onTap: () {
              Navigator.of(context).pop();
              connect(
                context,
                entry.code,
                password: entry.password,
              );
            },
          );
        },
      ),
    );
  }
}
