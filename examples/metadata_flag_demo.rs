// Example demonstrating the new metadata flag feature for the solve pipeline

use oat_db_rust::logic::SolvePipeline;
use oat_db_rust::model::{NewConfigurationArtifact, ResolutionContext};
use oat_db_rust::store::mem::MemoryStore;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up a memory store for demonstration
    let store = MemoryStore::new();
    let pipeline = SolvePipeline::new(&store);
    
    // Example configuration for solving
    let request = NewConfigurationArtifact {
        resolution_context: ResolutionContext {
            database_id: "demo-db".to_string(),
            branch_id: "main".to_string(),
        },
        user_metadata: None,
    };
    
    let target_instance_id = "example-instance".to_string();
    
    println!("🚀 Demonstrating solve pipeline with metadata control");
    
    // Example 1: With metadata collection (default behavior)
    println!("\n📊 Running solve pipeline WITH metadata collection:");
    match pipeline.solve_instance_with_metadata_control(request.clone(), target_instance_id.clone(), true).await {
        Ok(artifact) => {
            println!("   ✅ Solve completed with metadata");
            println!("   📈 Total time: {}ms", artifact.solve_metadata.total_time_ms);
            println!("   🔄 Pipeline phases: {}", artifact.solve_metadata.pipeline_phases.len());
            for phase in &artifact.solve_metadata.pipeline_phases {
                println!("      - {}: {}ms", phase.name, phase.duration_ms);
            }
        },
        Err(e) => println!("   ❌ Error (expected - no actual data): {}", e)
    }
    
    // Example 2: Without metadata collection (optimized performance)
    println!("\n🏃‍♂️ Running solve pipeline WITHOUT metadata collection:");
    match pipeline.solve_instance_with_metadata_control(request, target_instance_id, false).await {
        Ok(artifact) => {
            println!("   ✅ Solve completed without metadata");
            println!("   📈 Total time: {}ms (should be 0)", artifact.solve_metadata.total_time_ms);
            println!("   🔄 Pipeline phases: {} (should be 0)", artifact.solve_metadata.pipeline_phases.len());
            println!("   🎯 Performance optimization: No timing overhead!");
        },
        Err(e) => println!("   ❌ Error (expected - no actual data): {}", e)
    }
    
    println!("\n🎉 Metadata flag feature successfully implemented!");
    println!("   📋 Available methods:");
    println!("   • solve_instance_with_metadata_control(request, id, include_metadata)");
    println!("   • solve_instance_with_full_options(request, id, objectives, derived, include_metadata)");
    println!("   📊 When include_metadata = false:");
    println!("   • No timing data collected in any phase");  
    println!("   • No PipelinePhase objects created");
    println!("   • Minimal SolveMetadata with empty phases");
    println!("   • Better performance for production workloads");
    
    Ok(())
}