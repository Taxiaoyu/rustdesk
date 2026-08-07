import 'dart:convert';
import 'package:flutter/material.dart';
import 'package:flutter_hbb/common.dart';
import 'package:http/http.dart' as http;

/// RemoteVault 服务器地址
const kRemoteVaultUrl = 'http://118.31.42.84:13002';

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
  } catch (_) {}
  return [];
}

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
      if (entries.isEmpty) {
        _error = 'No RustDesk entries found in RemoteVault';
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    return SimpleDialog(
      title: Row(
        children: [
          const Text('RemoteVault'),
          const Spacer(),
          GestureDetector(
            onTap: () {
              setState(() {
                _loading = true;
                _error = null;
              });
              _load();
            },
            child: const Padding(
              padding: EdgeInsets.all(8.0),
              child: Icon(Icons.refresh, size: 18),
            ),
          ),
          GestureDetector(
            onTap: () => Navigator.of(context).pop(),
            child: const Padding(
              padding: EdgeInsets.all(8.0),
              child: Icon(Icons.close, size: 18),
            ),
          ),
        ],
      ),
      children: _buildContent(),
    );
  }

  List<Widget> _buildContent() {
    if (_loading) {
      return [
        const SizedBox(
          height: 80,
          child: Center(child: CircularProgressIndicator(strokeWidth: 2)),
        ),
      ];
    }

    if (_error != null && _entries.isEmpty) {
      return [
        SizedBox(
          height: 60,
          child: Center(
            child: Padding(
              padding: const EdgeInsets.all(16.0),
              child: Text(_error!, style: const TextStyle(color: Colors.grey)),
            ),
          ),
        ),
      ];
    }

    return _entries.map((entry) {
      return SimpleDialogOption(
        onPressed: () {
          Navigator.of(context).pop();
          connect(context, entry.code, password: entry.password);
        },
        child: ListTile(
          dense: true,
          leading: Container(
            width: 32,
            height: 32,
            decoration: BoxDecoration(
              color: Colors.orange.withOpacity(0.1),
              borderRadius: BorderRadius.circular(8),
            ),
            child: const Center(child: Text('R')),
          ),
          title: Text(entry.projectName),
          subtitle: Text(entry.code),
          trailing: entry.password != null && entry.password!.isNotEmpty
              ? const Icon(Icons.vpn_key, size: 14, color: Colors.orange)
              : null,
        ),
      );
    }).toList();
  }
}
