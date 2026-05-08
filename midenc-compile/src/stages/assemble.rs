use alloc::vec::Vec;

use miden_mast_package::{Package, Section, SectionId};

use super::*;

/// The artifact produced by the full compiler pipeline.
///
/// The type of artifact depends on what outputs were requested, and what options were specified.
pub enum Artifact {
    Lowered(CodegenOutput),
    Assembled(Package),
}
impl Artifact {
    pub fn unwrap_mast(self) -> Package {
        match self {
            Self::Assembled(mast) => mast,
            Self::Lowered(_) => {
                panic!("expected 'mast' artifact, but assembler stage was not run")
            }
        }
    }
}

/// Perform assembly of the generated Miden Assembly, producing MAST
pub struct AssembleStage;

impl Stage for AssembleStage {
    type Input = CodegenOutput;
    type Output = Artifact;

    fn run(&mut self, input: Self::Input, context: Rc<Context>) -> CompilerResult<Self::Output> {
        use midenc_hir::formatter::DisplayHex;

        let session = context.session();
        if session.should_assemble() {
            log::debug!("assembling mast artifact");
            let mut mast = input.component.assemble(
                &input.link_libraries,
                &input.link_packages,
                input.account_component_metadata_bytes.as_deref(),
                session,
            )?;
            replace_debug_sections(&mut mast, input.debug_info_bytes);
            log::debug!(
                "successfully assembled mast artifact with digest {}",
                DisplayHex::new(&mast.digest().as_bytes())
            );
            Ok(Artifact::Assembled(mast))
        } else {
            log::debug!(
                "skipping assembly of mast package from masm artifact (should-assemble=false)"
            );
            Ok(Artifact::Lowered(input))
        }
    }
}

fn replace_debug_sections(
    package: &mut Package,
    debug_info_bytes: Option<(Vec<u8>, Vec<u8>, Vec<u8>)>,
) {
    let Some((types_bytes, sources_bytes, functions_bytes)) = debug_info_bytes else {
        return;
    };

    package.sections.retain(|section| {
        section.id != SectionId::DEBUG_TYPES
            && section.id != SectionId::DEBUG_SOURCES
            && section.id != SectionId::DEBUG_FUNCTIONS
    });
    package.sections.push(Section::new(SectionId::DEBUG_TYPES, types_bytes));
    package.sections.push(Section::new(SectionId::DEBUG_SOURCES, sources_bytes));
    package.sections.push(Section::new(SectionId::DEBUG_FUNCTIONS, functions_bytes));
}
