// Replays a render vector stream through the real C++ NativeRenderer on a headless Device and prints one line per
// readTarget / present: "<index> <kind> <target> <hash> <recorded hash>".
// usage: Replay <shader dir> <resource folder> <stream> [out.txt]
#include <cstdio>
#include <memory>
#include <optional>

#include "Assets/Resources.hpp"
#include "Render/Device.hpp"
#include "Render/NativeRenderer.hpp"
#include "Scene/Font.hpp"
#include "Scene/Text.hpp"
#include "Stream.hpp"

namespace {

void check(VkResult r, const char* what) {
    if (r != VK_SUCCESS) throw std::runtime_error(std::string(what) + " " + std::to_string(r));
}

void barrier(VkCommandBuffer cmd, VkImage image, VkImageLayout from, VkImageLayout to, VkPipelineStageFlags2 ss, VkAccessFlags2 sa, VkPipelineStageFlags2 ds, VkAccessFlags2 da) {
    VkImageMemoryBarrier2 b{VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER_2};
    b.srcStageMask = ss;
    b.srcAccessMask = sa;
    b.dstStageMask = ds;
    b.dstAccessMask = da;
    b.oldLayout = from;
    b.newLayout = to;
    b.srcQueueFamilyIndex = b.dstQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED;
    b.image = image;
    b.subresourceRange = {VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1};
    VkDependencyInfo dep{VK_STRUCTURE_TYPE_DEPENDENCY_INFO};
    dep.imageMemoryBarrierCount = 1;
    dep.pImageMemoryBarriers = &b;
    vkCmdPipelineBarrier2(cmd, &dep);
}

// present() into an offscreen B8G8R8A8_UNORM image in COLOR_ATTACHMENT_OPTIMAL, as a swapchain image would be, read back.
uint64_t presentHash(render::Device& device, render::NativeRenderer& renderer, uint32_t w, uint32_t h, scene::TargetName shown, float aspect) {
    const VkDevice d = device.device();
    VkImageCreateInfo info{VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO};
    info.imageType = VK_IMAGE_TYPE_2D;
    info.format = VK_FORMAT_B8G8R8A8_UNORM;
    info.extent = {w, h, 1};
    info.mipLevels = info.arrayLayers = 1;
    info.samples = VK_SAMPLE_COUNT_1_BIT;
    info.tiling = VK_IMAGE_TILING_OPTIMAL;
    info.usage = VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT | VK_IMAGE_USAGE_TRANSFER_DST_BIT | VK_IMAGE_USAGE_TRANSFER_SRC_BIT;
    VmaAllocationCreateInfo ia{};
    ia.usage = VMA_MEMORY_USAGE_AUTO_PREFER_DEVICE;
    VkImage image;
    VmaAllocation ialloc;
    check(vmaCreateImage(device.allocator(), &info, &ia, &image, &ialloc, nullptr), "image");
    VkBufferCreateInfo binfo{VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO};
    binfo.size = VkDeviceSize(w) * h * 4;
    binfo.usage = VK_BUFFER_USAGE_TRANSFER_DST_BIT;
    VmaAllocationCreateInfo ba{};
    ba.usage = VMA_MEMORY_USAGE_AUTO;
    ba.flags = VMA_ALLOCATION_CREATE_MAPPED_BIT | VMA_ALLOCATION_CREATE_HOST_ACCESS_RANDOM_BIT;
    VkBuffer buffer;
    VmaAllocation balloc;
    VmaAllocationInfo bres{};
    check(vmaCreateBuffer(device.allocator(), &binfo, &ba, &buffer, &balloc, &bres), "buffer");

    VkCommandPoolCreateInfo pinfo{VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO};
    pinfo.queueFamilyIndex = device.queueFamily();
    VkCommandPool pool;
    check(vkCreateCommandPool(d, &pinfo, nullptr, &pool), "pool");
    VkCommandBufferAllocateInfo cinfo{VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO};
    cinfo.commandPool = pool;
    cinfo.level = VK_COMMAND_BUFFER_LEVEL_PRIMARY;
    cinfo.commandBufferCount = 1;
    VkCommandBuffer cmd;
    check(vkAllocateCommandBuffers(d, &cinfo, &cmd), "cmd");
    VkCommandBufferBeginInfo begin{VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO};
    begin.flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT;
    check(vkBeginCommandBuffer(cmd, &begin), "begin");
    barrier(cmd, image, VK_IMAGE_LAYOUT_UNDEFINED, VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL, VK_PIPELINE_STAGE_2_COLOR_ATTACHMENT_OUTPUT_BIT, 0,
            VK_PIPELINE_STAGE_2_COLOR_ATTACHMENT_OUTPUT_BIT, VK_ACCESS_2_COLOR_ATTACHMENT_WRITE_BIT);
    renderer.present(cmd, image, {w, h}, shown, aspect);
    barrier(cmd, image, VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL, VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL, VK_PIPELINE_STAGE_2_COLOR_ATTACHMENT_OUTPUT_BIT,
            VK_ACCESS_2_COLOR_ATTACHMENT_WRITE_BIT, VK_PIPELINE_STAGE_2_TRANSFER_BIT, VK_ACCESS_2_TRANSFER_READ_BIT);
    VkBufferImageCopy copy{};
    copy.imageSubresource = {VK_IMAGE_ASPECT_COLOR_BIT, 0, 0, 1};
    copy.imageExtent = {w, h, 1};
    vkCmdCopyImageToBuffer(cmd, image, VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL, buffer, 1, &copy);
    check(vkEndCommandBuffer(cmd), "end");
    VkCommandBufferSubmitInfo cs{VK_STRUCTURE_TYPE_COMMAND_BUFFER_SUBMIT_INFO};
    cs.commandBuffer = cmd;
    VkSubmitInfo2 s{VK_STRUCTURE_TYPE_SUBMIT_INFO_2};
    s.commandBufferInfoCount = 1;
    s.pCommandBufferInfos = &cs;
    check(vkQueueSubmit2(device.queue(), 1, &s, VK_NULL_HANDLE), "submit");
    check(vkQueueWaitIdle(device.queue()), "wait");
    check(vmaInvalidateAllocation(device.allocator(), balloc, 0, VK_WHOLE_SIZE), "invalidate");
    const uint64_t hash = stream::fnv(static_cast<const uint8_t*>(bres.pMappedData), size_t(w) * h * 4);
    vkDestroyCommandPool(d, pool, nullptr);
    vmaDestroyBuffer(device.allocator(), buffer, balloc);
    vmaDestroyImage(device.allocator(), image, ialloc);
    return hash;
}

}  // namespace

int main(int argc, char** argv) {
    if (argc != 4 && argc != 5) {
        std::fprintf(stderr, "usage: Replay <shader dir> <resource folder> <stream> [out.txt]\n");
        return 1;
    }
    try {
        const assets::AssetSet set = assets::decodeFolder(argv[2]);
        const scene::Font font(set.find("FNTOSD")->data);
        stream::Reader in(argv[3]);
        std::FILE* out = argc == 5 ? std::fopen(argv[4], "w") : stdout;
        render::Device device(nullptr, {false});
        render::NativeRenderer renderer(device, argv[1]);
        size_t index = 0, reads = 0, matched = 0, recorded = 0, glyphImages = 0, glyphWrong = 0, draws = 0;
        while (!in.done()) {
            const uint8_t tag = in.u8();
            switch (tag) {
            case stream::Configure: {
                const uint32_t w = in.u32(), h = in.u32(), s = in.u32();
                renderer.configure({w, h, s});
                break;
            }
            case stream::SetTexture: {
                const auto ts = static_cast<scene::TextureSet>(in.i32());
                const int32_t n = in.i32();
                const uint32_t w = in.u32(), h = in.u32();
                std::vector<uint8_t> rgba(size_t(w) * h * 4);
                in.raw(rgba.data(), rgba.size());
                renderer.setTexture(ts, n, w, h, rgba);
                break;
            }
            case stream::GlyphCacheTag: {
                const scene::GlyphCache cache = in.glyphs();
                std::vector<uint8_t> image(in.u64());
                in.raw(image.data(), image.size());
                if (!image.empty()) {
                    ++glyphImages;
                    glyphWrong += scene::glyphCacheImage(font, cache) != image;
                }
                renderer.setGlyphCache(font, cache);
                break;
            }
            case stream::Draw: {
                renderer.draw(in.frame());
                ++draws;
                break;
            }
            case stream::ReadTarget: {
                const auto t = static_cast<scene::TargetName>(in.i32());
                const uint64_t want = in.u64();
                const std::vector<uint8_t> r = renderer.readTarget(t);
                const uint64_t got = stream::fnv(r.data(), r.size());
                std::fprintf(out, "%zu read %d %016llx %016llx\n", index++, int(t), (unsigned long long)got, (unsigned long long)want);
                ++reads;
                if (want) {
                    ++recorded;
                    matched += want == got;
                }
                break;
            }
            case stream::Label: {
                std::fprintf(out, "# %s\n", in.str().c_str());
                break;
            }
            case stream::Present: {
                const uint32_t w = in.u32(), h = in.u32();
                const auto shown = static_cast<scene::TargetName>(in.i32());
                const float aspect = in.f32();
                const uint64_t want = in.u64();
                const uint64_t got = presentHash(device, renderer, w, h, shown, aspect);
                std::fprintf(out, "%zu present %d %016llx %016llx\n", index++, int(shown), (unsigned long long)got, (unsigned long long)want);
                ++reads;
                if (want) {
                    ++recorded;
                    matched += want == got;
                }
                break;
            }
            default: throw std::runtime_error("unknown tag " + std::to_string(tag));
            }
        }
        if (out != stdout) std::fclose(out);
        std::printf("replay: %zu draws, %zu reads, %zu with a recorded hash, %zu equal; glyph images %zu, %zu differ from glyphCacheImage\n", draws, reads, recorded, matched,
                    glyphImages, glyphWrong);
        return matched == recorded && glyphWrong == 0 ? 0 : 2;
    } catch (const std::exception& e) {
        std::fprintf(stderr, "exception: %s\n", e.what());
        return 1;
    }
}
