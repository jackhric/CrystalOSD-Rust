#define IMGUI_IMPL_VULKAN_NO_PROTOTYPES
#include "imgui.h"
#include "backends/imgui_impl_sdl3.cpp"
#include "backends/imgui_impl_vulkan.cpp"

namespace {
VkFormat swapchainFormat;
PFN_vkGetInstanceProcAddr getInstanceProcAddr;
}

extern "C" {

bool crystal_imgui_init(SDL_Window* window, PFN_vkGetInstanceProcAddr loader, VkInstance instance, VkPhysicalDevice physicalDevice, VkDevice device, uint32_t queueFamily,
                        VkQueue queue, uint32_t swapchainImageCount, VkFormat format) {
    IMGUI_CHECKVERSION();
    ImGui::CreateContext();
    ImGui::GetIO().IniFilename = nullptr;
    ImGui_ImplSDL3_InitForVulkan(window);
    getInstanceProcAddr = loader;
    if (!ImGui_ImplVulkan_LoadFunctions(VK_API_VERSION_1_4, [](const char* name, void* user) { return getInstanceProcAddr(static_cast<VkInstance>(user), name); }, instance))
        return false;
    swapchainFormat = format;
    ImGui_ImplVulkan_InitInfo imgui{};
    imgui.ApiVersion = VK_API_VERSION_1_4;
    imgui.Instance = instance;
    imgui.PhysicalDevice = physicalDevice;
    imgui.Device = device;
    imgui.QueueFamily = queueFamily;
    imgui.Queue = queue;
    imgui.DescriptorPoolSize = IMGUI_IMPL_VULKAN_MINIMUM_SAMPLED_IMAGE_POOL_SIZE;
    imgui.MinImageCount = 2;
    imgui.ImageCount = swapchainImageCount > 3 ? swapchainImageCount : 3;
    imgui.UseDynamicRendering = true;
    imgui.PipelineInfoMain.PipelineRenderingCreateInfo = {};
    imgui.PipelineInfoMain.PipelineRenderingCreateInfo.sType = VK_STRUCTURE_TYPE_PIPELINE_RENDERING_CREATE_INFO;
    imgui.PipelineInfoMain.PipelineRenderingCreateInfo.colorAttachmentCount = 1;
    imgui.PipelineInfoMain.PipelineRenderingCreateInfo.pColorAttachmentFormats = &swapchainFormat;
    return ImGui_ImplVulkan_Init(&imgui);
}

void crystal_imgui_process_event(const SDL_Event* event) { ImGui_ImplSDL3_ProcessEvent(event); }

bool crystal_imgui_wants_keyboard() { return ImGui::GetIO().WantCaptureKeyboard; }

void crystal_imgui_new_frame() {
    ImGui_ImplVulkan_NewFrame();
    ImGui_ImplSDL3_NewFrame();
    ImGui::NewFrame();
}

void crystal_imgui_render() { ImGui::Render(); }

void crystal_imgui_draw(VkCommandBuffer cmd) { ImGui_ImplVulkan_RenderDrawData(ImGui::GetDrawData(), cmd); }

void crystal_imgui_shutdown() {
    ImGui_ImplVulkan_Shutdown();
    ImGui_ImplSDL3_Shutdown();
    ImGui::DestroyContext();
}
}
