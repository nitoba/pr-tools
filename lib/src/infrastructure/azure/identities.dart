import 'package:better_effect/better_effect.dart';

import '../../app/app_effect.dart';
import 'client.dart';
import 'types.dart';

abstract interface class AzureIdentityClient {
  AppEffect<AzureIdentity> resolve(String value);
}

final class AzureIdentityClientLive implements AzureIdentityClient {
  const AzureIdentityClientLive();

  @override
  AppEffect<AzureIdentity> resolve(String value) => Effect.result((use) async {
    final normalized = value.trim();
    if (normalized.isEmpty) {
      use.fail(const AzurePayloadError('Reviewer não informado.'));
    }

    final options = use<AzureClientOptions>();
    final query = azureQuery(_identityParameters(normalized));
    final response = await use.unwrap(
      use<AzureDevOpsClient>().request(
        AzureRequest(
          withApiVersion('/_apis/identities?$query'),
          baseUrl: _identityBaseUrl(options),
        ),
      ),
    );
    final data = objectMap(response.data);
    final rawItems = data?['value'];
    if (rawItems is! List) {
      use.fail(
        const AzurePayloadError(
          'Azure não retornou identidades válidas para o reviewer.',
        ),
      );
    }

    for (final rawItem in rawItems) {
      final item = objectMap(rawItem);
      if (item == null) continue;
      final identity = AzureIdentity.fromJson(item);
      if (identity.id case final id? when id.trim().isNotEmpty) {
        return AzureIdentity(id: id);
      }
    }
    use.fail(
      AzurePayloadError(
        'Reviewer não encontrado no Azure DevOps: $normalized.',
      ),
    );
  });
}

Map<String, String> _identityParameters(String value) {
  final isId = RegExp(
    r'^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$',
  ).hasMatch(value);
  return isId
      ? {'identityIds': value, 'queryMembership': 'None'}
      : {
          'searchFilter': 'General',
          'filterValue': value,
          'queryMembership': 'None',
        };
}

String _identityBaseUrl(AzureClientOptions options) =>
    options.baseUrl ??
    'https://vssps.dev.azure.com/${pathSegment(options.organization)}';
