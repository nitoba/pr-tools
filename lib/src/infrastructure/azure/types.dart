final class AzureRepository {
  const AzureRepository({
    required this.id,
    this.name,
    this.remoteUrl,
    this.webUrl,
  });

  factory AzureRepository.fromJson(Map<String, Object?> json) {
    return AzureRepository(
      id: _string(json, 'id'),
      name: _nullableString(json, 'name'),
      remoteUrl: _nullableString(json, 'remoteUrl'),
      webUrl: _nullableString(json, 'webUrl'),
    );
  }

  final String id;
  final String? name;
  final String? remoteUrl;
  final String? webUrl;
}

final class AzureIdentity {
  const AzureIdentity({this.id, this.uniqueName, this.displayName});

  factory AzureIdentity.fromJson(Map<String, Object?> json) {
    return AzureIdentity(
      id: _nullableString(json, 'id'),
      uniqueName: _nullableString(json, 'uniqueName'),
      displayName: _nullableString(json, 'displayName'),
    );
  }

  final String? id;
  final String? uniqueName;
  final String? displayName;

  Map<String, Object?> toJson() => {
    if (id != null) 'id': id,
    if (uniqueName != null) 'uniqueName': uniqueName,
    if (displayName != null) 'displayName': displayName,
  };
}

final class AzureResourceRef {
  const AzureResourceRef({required this.id, this.url});

  factory AzureResourceRef.fromJson(Map<String, Object?> json) {
    return AzureResourceRef(
      id: '${json['id'] ?? ''}',
      url: _nullableString(json, 'url'),
    );
  }

  final String id;
  final String? url;

  Map<String, Object?> toJson() => {'id': id, if (url != null) 'url': url};
}

final class AzureCommitRef {
  const AzureCommitRef({required this.commitId});

  factory AzureCommitRef.fromJson(Map<String, Object?> json) =>
      AzureCommitRef(commitId: _string(json, 'commitId'));

  final String commitId;
}

final class AzurePullRequest {
  const AzurePullRequest({
    required this.pullRequestId,
    required this.title,
    required this.description,
    required this.sourceRefName,
    required this.targetRefName,
    this.status,
    this.closedDate,
    this.lastMergeSourceCommit,
    this.url,
    this.webUrl,
    this.repository,
    this.reviewers,
    this.workItemRefs,
  });

  factory AzurePullRequest.fromJson(Map<String, Object?> json) {
    return AzurePullRequest(
      pullRequestId: _integer(json['pullRequestId']),
      title: _string(json, 'title'),
      description: _string(json, 'description'),
      sourceRefName: _string(json, 'sourceRefName'),
      targetRefName: _string(json, 'targetRefName'),
      status: _nullableString(json, 'status'),
      closedDate: _dateTime(json['closedDate']),
      lastMergeSourceCommit: objectMap(json['lastMergeSourceCommit']) == null
          ? null
          : AzureCommitRef.fromJson(objectMap(json['lastMergeSourceCommit'])!),
      url: _nullableString(json, 'url'),
      webUrl: _nullableString(json, 'webUrl'),
      repository: objectMap(json['repository']) == null
          ? null
          : AzureRepository.fromJson(objectMap(json['repository'])!),
      reviewers: objectList(json['reviewers'])
          ?.map((value) => AzureIdentity.fromJson(value))
          .toList(growable: false),
      workItemRefs: objectList(json['workItemRefs'])
          ?.map((value) => AzureResourceRef.fromJson(value))
          .toList(growable: false),
    );
  }

  final int pullRequestId;
  final String title;
  final String description;
  final String sourceRefName;
  final String targetRefName;
  final String? status;
  final DateTime? closedDate;
  final AzureCommitRef? lastMergeSourceCommit;
  final String? url;
  final String? webUrl;
  final AzureRepository? repository;
  final List<AzureIdentity>? reviewers;
  final List<AzureResourceRef>? workItemRefs;
}

final class CreatePullRequestInput {
  const CreatePullRequestInput({
    required this.title,
    required this.description,
    required this.sourceRefName,
    required this.targetRefName,
    this.reviewers,
    this.workItemRefs,
  });

  final String title;
  final String description;
  final String sourceRefName;
  final String targetRefName;
  final List<AzureIdentity>? reviewers;
  final List<AzureResourceRef>? workItemRefs;

  Map<String, Object?> toJson() => {
    'title': title,
    'description': description,
    'sourceRefName': sourceRefName,
    'targetRefName': targetRefName,
    if (reviewers != null)
      'reviewers': reviewers!
          .map((value) => {...value.toJson(), 'isRequired': true})
          .toList(),
    if (workItemRefs != null)
      'workItemRefs': workItemRefs!.map((value) => value.toJson()).toList(),
  };
}

final class AzureWorkItem {
  const AzureWorkItem({
    required this.id,
    required this.fields,
    this.rev,
    this.url,
  });

  factory AzureWorkItem.fromJson(Map<String, Object?> json) {
    final fields = objectMap(json['fields']) ?? const <String, Object?>{};
    return AzureWorkItem(
      id: _integer(json['id']),
      rev: _nullableInteger(json['rev']),
      url: _nullableString(json, 'url'),
      fields: fields,
    );
  }

  final int id;
  final int? rev;
  final String? url;
  final Map<String, Object?> fields;
}

final class CreateTestCaseInput {
  const CreateTestCaseInput({
    required this.title,
    this.descriptionHtml,
    this.stepsXml,
    this.areaPath,
    this.parentId,
    this.iterationPath,
    this.priority,
    this.team,
    this.program,
    this.assignedTo,
  });

  final String title;
  final String? descriptionHtml;
  final String? stepsXml;
  final String? areaPath;
  final int? parentId;
  final String? iterationPath;
  final num? priority;
  final String? team;
  final String? program;
  final String? assignedTo;
}

final class AzurePullRequestIteration {
  const AzurePullRequestIteration({required this.id});

  factory AzurePullRequestIteration.fromJson(Map<String, Object?> json) {
    return AzurePullRequestIteration(id: _integer(json['id']));
  }

  final int id;
}

final class AzurePullRequestChange {
  const AzurePullRequestChange({required this.changeType, required this.path});

  factory AzurePullRequestChange.fromJson(Map<String, Object?> json) {
    final item = objectMap(json['item']);
    return AzurePullRequestChange(
      changeType: _string(json, 'changeType'),
      path: item == null ? '' : _string(item, 'path'),
    );
  }

  final String changeType;
  final String path;
}

Map<String, Object?>? objectMap(Object? value) {
  if (value is! Map) return null;
  return value.map<String, Object?>((key, value) => MapEntry('$key', value));
}

List<Map<String, Object?>>? objectList(Object? value) {
  if (value is! List) return null;
  return value
      .map(objectMap)
      .whereType<Map<String, Object?>>()
      .toList(growable: false);
}

String _string(Map<String, Object?> json, String key) {
  final value = json[key];
  if (value == null) return '';
  if (value is String) return value;
  throw FormatException('Campo Azure `$key` deveria ser texto.');
}

String? _nullableString(Map<String, Object?> json, String key) {
  final value = json[key];
  if (value == null || value is String) return value as String?;
  throw FormatException('Campo Azure `$key` deveria ser texto.');
}

DateTime? _dateTime(Object? value) {
  if (value == null) return null;
  if (value is! String) {
    throw const FormatException('Campo Azure `closedDate` deveria ser texto.');
  }
  final parsed = DateTime.tryParse(value);
  if (parsed == null) {
    throw FormatException('Data Azure inválida: $value.');
  }
  return parsed;
}

int _integer(Object? value) {
  final number = _number(value);
  if (number == null ||
      !number.isFinite ||
      number != number.truncateToDouble()) {
    return 0;
  }
  return number.toInt();
}

int? _nullableInteger(Object? value) {
  if (value == null) return null;
  return _integer(value);
}

double? _number(Object? value) {
  if (value is num) return value.toDouble();
  if (value is String) return double.tryParse(value);
  return null;
}

final class AzureUnit {
  const AzureUnit();
}
