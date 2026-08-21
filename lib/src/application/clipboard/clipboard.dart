import '../../app/app_effect.dart';

abstract interface class Clipboard {
  AppEffect<bool> copy(String value);
}
