// DisplayConfig DPI helper adapted from the Unlicense sample at
// https://github.com/lihas/windows-DPI-scaling-sample/tree/738ac18b7a7ce2d8fdc157eb825de9cb5eee0448
#include <Windows.h>

#include <algorithm>
#include <array>
#include <cstdint>
#include <iostream>
#include <string>
#include <vector>

namespace {
constexpr std::array<std::uint32_t, 12> kDpiValues = {
    100, 125, 150, 175, 200, 225, 250, 300, 350, 400, 450, 500,
};

enum class CustomDeviceInfoType : int {
  GetDpiScale = -3,
  SetDpiScale = -4,
};

struct DpiScaleGet {
  DISPLAYCONFIG_DEVICE_INFO_HEADER header;
  std::int32_t minimum_relative;
  std::int32_t current_relative;
  std::int32_t maximum_relative;
};

struct DpiScaleSet {
  DISPLAYCONFIG_DEVICE_INFO_HEADER header;
  std::int32_t scale_relative;
};

struct DpiInfo {
  std::uint32_t current;
  std::uint32_t recommended;
  std::uint32_t maximum;
};

bool active_source(LUID &adapter, UINT32 &source) {
  UINT32 path_count = 0;
  UINT32 mode_count = 0;
  if (GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &path_count,
                                  &mode_count) != ERROR_SUCCESS ||
      path_count != 1) {
    return false;
  }
  std::vector<DISPLAYCONFIG_PATH_INFO> paths(path_count);
  std::vector<DISPLAYCONFIG_MODE_INFO> modes(mode_count);
  if (QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS, &path_count, paths.data(),
                         &mode_count, modes.data(), nullptr) != ERROR_SUCCESS) {
    return false;
  }
  adapter = paths[0].sourceInfo.adapterId;
  source = paths[0].sourceInfo.id;
  return true;
}

bool dpi_info(const LUID adapter, const UINT32 source, DpiInfo &info) {
  DpiScaleGet packet{};
  static_assert(sizeof(packet) == 0x20);
  packet.header.type = static_cast<DISPLAYCONFIG_DEVICE_INFO_TYPE>(
      CustomDeviceInfoType::GetDpiScale);
  packet.header.size = sizeof(packet);
  packet.header.adapterId = adapter;
  packet.header.id = source;
  if (DisplayConfigGetDeviceInfo(&packet.header) != ERROR_SUCCESS) {
    return false;
  }
  const auto minimum_index = static_cast<std::size_t>(-packet.minimum_relative);
  const auto current_index = static_cast<std::int64_t>(minimum_index) +
                             packet.current_relative;
  const auto maximum_index = static_cast<std::int64_t>(minimum_index) +
                             packet.maximum_relative;
  if (minimum_index >= kDpiValues.size() || current_index < 0 ||
      maximum_index < 0 ||
      static_cast<std::size_t>(maximum_index) >= kDpiValues.size()) {
    return false;
  }
  info = {kDpiValues[static_cast<std::size_t>(current_index)],
          kDpiValues[minimum_index],
          kDpiValues[static_cast<std::size_t>(maximum_index)]};
  return true;
}

bool set_dpi(const LUID adapter, const UINT32 source,
             const std::uint32_t requested, const DpiInfo &before) {
  const auto requested_it =
      std::find(kDpiValues.begin(), kDpiValues.end(), requested);
  const auto recommended_it =
      std::find(kDpiValues.begin(), kDpiValues.end(), before.recommended);
  if (requested_it == kDpiValues.end() || recommended_it == kDpiValues.end() ||
      requested > before.maximum) {
    return false;
  }
  DpiScaleSet packet{};
  static_assert(sizeof(packet) == 0x18);
  packet.header.type = static_cast<DISPLAYCONFIG_DEVICE_INFO_TYPE>(
      CustomDeviceInfoType::SetDpiScale);
  packet.header.size = sizeof(packet);
  packet.header.adapterId = adapter;
  packet.header.id = source;
  packet.scale_relative = static_cast<std::int32_t>(requested_it - recommended_it);
  return DisplayConfigSetDeviceInfo(&packet.header) == ERROR_SUCCESS;
}
} // namespace

int main(int argc, char **argv) {
  if (argc != 2) {
    std::cerr << "usage: windows-dpi-profile PERCENT\n";
    return 2;
  }
  const auto requested = static_cast<std::uint32_t>(std::stoul(argv[1]));
  LUID adapter{};
  UINT32 source = 0;
  DpiInfo before{};
  if (!active_source(adapter, source) || !dpi_info(adapter, source, before) ||
      !set_dpi(adapter, source, requested, before)) {
    std::cerr << "failed to set the single active display DPI profile\n";
    return 2;
  }
  Sleep(1500);
  DpiInfo after{};
  if (!dpi_info(adapter, source, after) || after.current != requested) {
    std::cerr << "Windows did not apply requested DPI profile\n";
    return 1;
  }
  std::cout << "{\"schema_version\":1,\"requested_percent\":" << requested
            << ",\"before_percent\":" << before.current
            << ",\"applied_percent\":" << after.current
            << ",\"recommended_percent\":" << after.recommended
            << ",\"maximum_percent\":" << after.maximum << "}\n";
  return 0;
}
