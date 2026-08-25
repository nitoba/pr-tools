import '../../app/app_effect.dart';
import '../../app/cli_options.dart';
import 'config_models.dart';

abstract interface class ConfigRuntime {
  ConfigPaths get paths;

  Map<String, String> get environment;

  bool get interactive;
}

abstract interface class ConfigFileSystem {
  AppEffect<bool> exists(String path);

  AppEffect<String> createDirectory(String path);

  AppEffect<String> read(String path);

  AppEffect<String> write(String path, String contents, {required int mode});
}

abstract interface class ConfigService {
  AppEffect<Config> load(CliOptions options);

  AppEffect<ConfigInitialization> initialize();
}
