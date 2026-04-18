fn main() {
    // Icon embedding via resource compilers (winres, embed-resource) doesn't
    // work reliably with the gnullvm toolchain. The window icon is set at
    // runtime via winit instead — see app.rs.
}
